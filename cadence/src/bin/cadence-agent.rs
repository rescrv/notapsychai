//! Telegram/stdin rhythm management agent.
//!
//! This tool provides a chat interface with an AI agent to help manage recurring rhythms and
//! habits. The conversation operates on a YAML cadence document and is driven by the durable
//! transport loop from `claudius-telegram`.
//!
//! Usage: `cadence-agent [--yaml DIR] [--model MODEL] [--import-request-log PATH] [--stdin]`
//!
//! Telegram mode reads `--token` or `TELEGRAM_BOT_TOKEN`; either may be a raw bot
//! token or `file://PATH`.

use std::any::Any;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration as StdDuration;

use arrrg::CommandLine;
use cadence_agent::{
    CadenceDocument, ProactiveCheckInPlan, RhythmID, RhythmInput, RhythmUpdate, TodayRenderOptions,
    add_rhythm, build_rhythm, convergence_response, default_file_path, defer_rhythm, delete_rhythm,
    delinquent_items, document_timezone, format_rhythm, list_rhythms, load_document,
    mark_rhythm_done, next_proactive_due_at_after, parse_date, parse_rhythm_id, parse_time,
    plan_scheduled_proactive_check_in, plan_startup_proactive_check_in, proactive_due_groups,
    record_proactive_check_in_plan, render_delinquent_items, render_schedule_items,
    render_today_items, rhythm_kind_name, save_document, schedule_items, set_spoons, today_items,
    update_rhythm,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use chrono_tz::Tz;
use claudius::Agent;
use claudius::AgentStreamContext;
use claudius::Anthropic;
use claudius::Budget;
use claudius::Content;
use claudius::ContentBlock;
use claudius::IntermediateToolResult;
use claudius::Message;
use claudius::MessageParam;
use claudius::MessageParamContent;
use claudius::MessageRole;
use claudius::Model;
use claudius::Renderer;
use claudius::StopReason;
use claudius::SystemPrompt;
use claudius::Tool;
use claudius::ToolCallback;
use claudius::ToolParam;
use claudius::ToolResult;
use claudius::ToolResultBlock;
use claudius::ToolResultBlockContent;
use claudius::ToolUnionParam;
use claudius::ToolUseBlock;
use claudius::TurnOutcome;
use claudius::chat::ChatConfig;
use claudius_telegram::{
    ChatId, ChatTransport, Error as TelegramError, FileStateStore, Inbound, LoopConfig,
    OutboxStatus, PreFilter, StateStore, StdinTransport, SyntheticTurnConfig, TelegramTransport,
    TransportState, known_chats, run_agent_loop, run_synthetic_user_turn,
};
use serde_json::{Value, json};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::Notify;

const DEFAULT_SYSTEM_PROMPT: &str = include_str!("../../prompts/default-system.md");

////////////////////////////////////////////// Config //////////////////////////////////////////////

#[derive(arrrg_derive::CommandLine, Debug, Default, Eq, PartialEq)]
struct CliOptions {
    #[arrrg(optional, "Directory containing per-user YAML files", "DIR")]
    yaml: Option<String>,
    #[arrrg(
        optional,
        "Anthropic model to use [default: claude-haiku-4-5]",
        "MODEL"
    )]
    model: Option<String>,
    #[arrrg(
        optional,
        "Import compact request logs from a transport state JSON and exit",
        "PATH"
    )]
    import_request_log: Option<String>,
    #[arrrg(optional, "Override the default mutation cap per turn", "N")]
    mutation_cap: Option<usize>,
    #[arrrg(flag, "Use stdin/stdout transport instead of Telegram")]
    stdin: bool,
    #[arrrg(optional, "Durable transport state JSON path", "PATH")]
    state: Option<String>,
    #[arrrg(
        optional,
        "Telegram bot token or file://PATH (default: $TELEGRAM_BOT_TOKEN)",
        "TOKEN"
    )]
    token: Option<String>,
    #[arrrg(optional, "Telegram long-poll timeout in seconds", "SECS")]
    poll_timeout: Option<u64>,
    #[arrrg(flag, "Call deleteWebhook on startup before long-polling")]
    delete_webhook_on_start: bool,
    #[arrrg(
        optional,
        "Reset chat history before a turn when it exceeds this many messages; 0 resets every turn",
        "N"
    )]
    history_message_threshold: Option<usize>,
    #[arrrg(flag, "Disable proactive Telegram check-ins at scheduled due times")]
    disable_proactive_checkins: bool,
    #[arrrg(
        optional,
        "Minimum time between proactive check-ins per chat [default: 1h]",
        "DURATION"
    )]
    proactive_min_interval: Option<String>,
}

//////////////////////////////////////////// Shared State //////////////////////////////////////////

/// Shared cadence document state for the agent.
type SharedDocument = Arc<Mutex<CadenceDocument>>;
type SharedFilePath = Arc<Mutex<PathBuf>>;

/// Maximum number of mutation tool calls allowed per agent turn.
const DEFAULT_MUTATION_CAP: usize = 20;

/// Default Anthropic model for the cadence agent.
const DEFAULT_MODEL: &str = "claude-haiku-4-5";

/// Request-log JSONL schema emitted beside each per-user YAML file.
const REQUEST_LOG_SCHEMA: &str = "cadence.request_log.v1";

/// Maximum persisted chat messages to send back to the model before resetting history.
const DEFAULT_HISTORY_MESSAGE_THRESHOLD: usize = 40;

/// Default minimum time between proactive check-ins for one chat.
const DEFAULT_PROACTIVE_MIN_INTERVAL_SECS: i64 = 60 * 60;

/// Number of days to scan for future proactive alarms.
const PROACTIVE_SCAN_HORIZON_DAYS: u32 = 90;

/// How often to rescan when no scheduled items exist in the scan horizon.
const PROACTIVE_IDLE_RESCAN_SECS: u64 = 60 * 60;

/// Tool names that do not count toward the mutation cap.
const READ_ONLY_TOOLS: &[&str] = &[
    "list_rhythms",
    "get_rhythm",
    "today",
    "schedule",
    "delinquent",
    "convergence",
    "show_spoons",
];

/// Tracks mutation tool calls within a single agent turn.
#[derive(Debug)]
struct MutationCounter {
    count: AtomicUsize,
    cap: usize,
}

impl MutationCounter {
    fn new(cap: usize) -> Self {
        Self {
            count: AtomicUsize::new(0),
            cap,
        }
    }

    /// Try to consume one mutation.  Returns Ok(()) if under the cap, or an
    /// error message if the cap has been reached.
    fn try_mutate(&self, tool_name: &str) -> Result<(), String> {
        let previous = self.count.fetch_add(1, Ordering::Relaxed);
        if previous >= self.cap {
            Err(format!(
                "Mutation cap reached ({} mutations this turn). \
                 Stop making changes and tell the user what you have done so far \
                 and what remains.  The user can ask you to continue. \
                 Tool '{}' was not executed.",
                self.cap, tool_name,
            ))
        } else {
            Ok(())
        }
    }

    /// Reset the counter for a new turn.
    fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
    }
}

/////////////////////////////////////////////// Agent //////////////////////////////////////////////

/// Agent for interactive cadence sessions.
struct CadenceAgent {
    document: SharedDocument,
    mutation_counter: Arc<MutationCounter>,
    file_path: SharedFilePath,
    config: ChatConfig,
    schedule_changed: Option<Arc<Notify>>,
    turn_source: String,
}

impl CadenceAgent {
    fn new(
        document: SharedDocument,
        mutation_counter: Arc<MutationCounter>,
        file_path: SharedFilePath,
        config: ChatConfig,
        schedule_changed: Option<Arc<Notify>>,
    ) -> Self {
        Self {
            document,
            mutation_counter,
            file_path,
            config,
            schedule_changed,
            turn_source: "live".to_string(),
        }
    }
}

fn personality_path_for_yaml_path(yaml_path: &Path) -> PathBuf {
    yaml_path.with_extension("md")
}

fn build_system_prompt_for_yaml_path(yaml_path: &Path) -> String {
    let mut prompt = DEFAULT_SYSTEM_PROMPT.to_string();
    let personality_path = personality_path_for_yaml_path(yaml_path);
    match fs::read_to_string(&personality_path) {
        Ok(personality) if !personality.trim().is_empty() => {
            prompt.push_str("\n\n<personality>\n");
            prompt.push_str(personality.trim());
            prompt.push_str("\n</personality>");
        }
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            eprintln!(
                "[cadence-agent] failed to read personality file {}: {err}",
                personality_path.display()
            );
        }
    }
    prompt
}

#[async_trait::async_trait]
impl Agent for CadenceAgent {
    async fn max_tokens(&self) -> u32 {
        self.config.max_tokens()
    }

    async fn model(&self) -> Model {
        self.config.model()
    }

    async fn system(&self) -> Option<SystemPrompt> {
        let yaml_path = self.file_path.lock().unwrap().clone();
        Some(SystemPrompt::String(build_system_prompt_for_yaml_path(
            &yaml_path,
        )))
    }

    fn stream_label(&self) -> String {
        "CadenceAgent".to_string()
    }

    fn caching_enabled(&self) -> bool {
        self.config.caching_enabled
    }

    async fn take_turn_streaming(
        &mut self,
        client: &Anthropic,
        messages: &mut Vec<MessageParam>,
        budget: &Arc<Budget>,
        renderer: &mut dyn Renderer,
        context: AgentStreamContext,
    ) -> Result<TurnOutcome, claudius::Error> {
        self.mutation_counter.reset();
        stamp_latest_user_message(messages, &self.document);
        let turn_start = latest_human_user_message_index(messages);
        let result = self
            .take_default_turn_streaming(client, messages, budget, renderer, context)
            .await;
        if let Ok(outcome) = &result
            && outcome.stop_reason != StopReason::ModelContextWindowExceeded
            && let Some(turn_start) = turn_start
        {
            let yaml_path = self.file_path.lock().unwrap().clone();
            if let Some(entry) = build_request_log_entry(
                &self.turn_source,
                chat_id_from_yaml_path(&yaml_path),
                &yaml_path,
                None,
                &self.config.model(),
                false,
                None,
                &messages[turn_start..],
            ) && let Err(err) = append_request_log_entry(&yaml_path, &entry)
            {
                eprintln!(
                    "[cadence-agent] failed to append request log for {}: {err}",
                    yaml_path.display()
                );
            }
        }
        result
    }

    async fn tools(&self) -> Vec<Arc<dyn Tool<Self>>> {
        vec![
            Arc::new(ListRhythmsTool::new(Arc::clone(&self.document))),
            Arc::new(GetRhythmTool::new(Arc::clone(&self.document))),
            Arc::new(TodayTool::new(Arc::clone(&self.document))),
            Arc::new(ScheduleTool::new(Arc::clone(&self.document))),
            Arc::new(DelinquentTool::new(Arc::clone(&self.document))),
            Arc::new(ConvergenceTool::new(Arc::clone(&self.document))),
            Arc::new(ShowSpoonsTool::new(Arc::clone(&self.document))),
            Arc::new(AddRhythmTool::new(
                Arc::clone(&self.document),
                Arc::clone(&self.file_path),
                self.schedule_changed.clone(),
            )),
            Arc::new(UpdateRhythmTool::new(
                Arc::clone(&self.document),
                Arc::clone(&self.file_path),
                self.schedule_changed.clone(),
            )),
            Arc::new(DeleteRhythmTool::new(
                Arc::clone(&self.document),
                Arc::clone(&self.file_path),
                self.schedule_changed.clone(),
            )),
            Arc::new(MarkDoneTool::new(
                Arc::clone(&self.document),
                Arc::clone(&self.file_path),
                self.schedule_changed.clone(),
            )),
            Arc::new(DeferRhythmTool::new(
                Arc::clone(&self.document),
                Arc::clone(&self.file_path),
                self.schedule_changed.clone(),
            )),
            Arc::new(SetSpoonsTool::new(
                Arc::clone(&self.document),
                Arc::clone(&self.file_path),
                self.schedule_changed.clone(),
            )),
        ]
    }

    async fn handle_tool_use_streaming(
        &mut self,
        client: &Anthropic,
        resp: &Message,
        renderer: &mut dyn Renderer,
        context: &AgentStreamContext,
    ) -> ControlFlow<Result<StopReason, claudius::Error>, Vec<ContentBlock>> {
        let tools_and_blocks = self.collect_tool_uses(resp).await;
        let mut tool_results = Vec::new();
        for (tool_use, tool) in tools_and_blocks.iter() {
            let is_read_only = READ_ONLY_TOOLS.contains(&tool_use.name.as_str());
            if !is_read_only && let Err(msg) = self.mutation_counter.try_mutate(&tool_use.name) {
                let block = ToolResultBlock {
                    tool_use_id: tool_use.id.clone(),
                    content: Some(ToolResultBlockContent::String(msg)),
                    is_error: Some(true),
                    cache_control: None,
                };
                tool_results.push(ContentBlock::ToolResult(block));
                continue;
            }
            let tool_context = context.child(format!("tool:{}", tool_use.name));
            let callback = tool.callback();
            let this = &*self;
            let intermediate = callback
                .compute_tool_result_streaming(client, this, tool_use, renderer, &tool_context)
                .await;
            match callback
                .apply_tool_result(client, self, tool_use, intermediate)
                .await
            {
                ControlFlow::Continue(result) => {
                    let block = match result {
                        Ok(block) => block,
                        Err(block) => block.with_error(true),
                    };
                    tool_results.push(ContentBlock::ToolResult(block));
                }
                ControlFlow::Break(err) => {
                    return ControlFlow::Break(Err(err));
                }
            }
        }
        ControlFlow::Continue(tool_results)
    }
}

#[derive(Clone)]
struct SharedCadenceAgent {
    inner: Arc<AsyncMutex<CadenceAgent>>,
    turn_source: String,
    document_context: Option<(SharedDocument, SharedFilePath)>,
}

impl SharedCadenceAgent {
    fn new(agent: CadenceAgent) -> Self {
        Self {
            inner: Arc::new(AsyncMutex::new(agent)),
            turn_source: "live".to_string(),
            document_context: None,
        }
    }

    fn with_turn_source(&self, turn_source: impl Into<String>) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            turn_source: turn_source.into(),
            document_context: self.document_context.clone(),
        }
    }

    fn with_document_context(&self, document: SharedDocument, file_path: SharedFilePath) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            turn_source: self.turn_source.clone(),
            document_context: Some((document, file_path)),
        }
    }
}

#[async_trait::async_trait]
impl Agent for SharedCadenceAgent {
    async fn max_tokens(&self) -> u32 {
        self.inner.lock().await.max_tokens().await
    }

    async fn model(&self) -> Model {
        self.inner.lock().await.model().await
    }

    async fn system(&self) -> Option<SystemPrompt> {
        self.inner.lock().await.system().await
    }

    fn stream_label(&self) -> String {
        "CadenceAgent".to_string()
    }

    fn caching_enabled(&self) -> bool {
        true
    }

    async fn take_turn_streaming(
        &mut self,
        client: &Anthropic,
        messages: &mut Vec<MessageParam>,
        budget: &Arc<Budget>,
        renderer: &mut dyn Renderer,
        context: AgentStreamContext,
    ) -> Result<TurnOutcome, claudius::Error> {
        let mut inner = self.inner.lock().await;
        let previous_source = std::mem::replace(&mut inner.turn_source, self.turn_source.clone());
        let previous_context = if let Some((document, file_path)) = self.document_context.as_ref() {
            Some((
                std::mem::replace(&mut inner.document, Arc::clone(document)),
                std::mem::replace(&mut inner.file_path, Arc::clone(file_path)),
            ))
        } else {
            None
        };

        let result = inner
            .take_turn_streaming(client, messages, budget, renderer, context)
            .await;

        inner.turn_source = previous_source;
        if let Some((document, file_path)) = previous_context {
            inner.document = document;
            inner.file_path = file_path;
        }

        result
    }
}

////////////////////////////////////////// Tool Helpers ////////////////////////////////////////////

/// Intermediate result for cadence tools.
struct CadenceToolResult {
    tool_use_id: String,
    result: Result<String, String>,
}

impl IntermediateToolResult for CadenceToolResult {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn apply_cadence_tool_result(intermediate: Box<dyn IntermediateToolResult>) -> ToolResult {
    let result = intermediate
        .as_any()
        .downcast_ref::<CadenceToolResult>()
        .expect("intermediate result type mismatch");

    let block = ToolResultBlock {
        tool_use_id: result.tool_use_id.clone(),
        content: Some(ToolResultBlockContent::String(
            result.result.clone().unwrap_or_else(|e| e.clone()),
        )),
        is_error: Some(result.result.is_err()),
        cache_control: None,
    };

    if result.result.is_ok() {
        ControlFlow::Continue(Ok(block))
    } else {
        ControlFlow::Continue(Err(block))
    }
}

fn json_schema(props: &[(&str, &str, Value)], required: &[&str]) -> Value {
    let mut properties = serde_json::Map::new();
    for (name, desc, schema) in props {
        let mut prop = schema.as_object().cloned().unwrap_or_default();
        prop.insert("description".to_string(), Value::String(desc.to_string()));
        properties.insert(name.to_string(), Value::Object(prop));
    }
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn custom_tool_param(name: &str, description: &str, input_schema: Value) -> ToolUnionParam {
    ToolUnionParam::CustomTool(ToolParam {
        name: name.to_string(),
        description: Some(description.to_string()),
        input_schema,
        cache_control: None,
        strict: None,
    })
}

fn tool_ok(tool_use_id: &str, message: impl Into<String>) -> Box<dyn IntermediateToolResult> {
    Box::new(CadenceToolResult {
        tool_use_id: tool_use_id.to_string(),
        result: Ok(message.into()),
    })
}

fn tool_err(tool_use_id: &str, message: impl Into<String>) -> Box<dyn IntermediateToolResult> {
    Box::new(CadenceToolResult {
        tool_use_id: tool_use_id.to_string(),
        result: Err(message.into()),
    })
}

fn get_required_str<'a>(tool_use: &'a ToolUseBlock, field: &str) -> Result<&'a str, String> {
    tool_use
        .input
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing or invalid '{field}'"))
}

fn get_optional_str<'a>(tool_use: &'a ToolUseBlock, field: &str) -> Option<&'a str> {
    tool_use.input.get(field).and_then(|v| v.as_str())
}

fn get_optional_u64(tool_use: &ToolUseBlock, field: &str) -> Option<u64> {
    tool_use.input.get(field).and_then(|v| v.as_u64())
}

fn get_required_u64(tool_use: &ToolUseBlock, field: &str) -> Result<u64, String> {
    tool_use
        .input
        .get(field)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("Missing or invalid '{field}'"))
}

fn get_required_rhythm_id<'a>(
    tool_use: &'a ToolUseBlock,
    field: &str,
) -> Result<(&'a str, RhythmID), String> {
    let id = get_required_str(tool_use, field)?;
    let rhythm_id = parse_rhythm_id(id).map_err(|e| e.to_string())?;
    Ok((id, rhythm_id))
}

fn save_document_or_tool_err(
    tool_use_id: &str,
    file_path: &Path,
    document: &CadenceDocument,
    action: &str,
) -> Result<(), Box<dyn IntermediateToolResult>> {
    save_document(file_path, document)
        .map_err(|e| tool_err(tool_use_id, format!("{action} but failed to save: {e}")))
}

fn save_current_document_or_tool_err(
    tool_use_id: &str,
    file_path: &SharedFilePath,
    document: &CadenceDocument,
    action: &str,
) -> Result<(), Box<dyn IntermediateToolResult>> {
    let file_path = file_path.lock().unwrap().clone();
    save_document_or_tool_err(tool_use_id, &file_path, document, action)
}

fn notify_schedule_changed(schedule_changed: &Option<Arc<Notify>>) {
    if let Some(schedule_changed) = schedule_changed {
        schedule_changed.notify_waiters();
    }
}

fn parse_rhythm_input(tool_use: &ToolUseBlock) -> Result<RhythmInput, String> {
    let dotw = get_optional_u64(tool_use, "dotw")
        .map(u8::try_from)
        .transpose()
        .map_err(|_| "dotw must fit within 0-255".to_string())?;
    let dotm = get_optional_u64(tool_use, "dotm")
        .map(u32::try_from)
        .transpose()
        .map_err(|_| "dotm must fit within 0-4294967295".to_string())?;
    let every_n_days = get_optional_u64(tool_use, "n")
        .map(u32::try_from)
        .transpose()
        .map_err(|_| "n must fit within 0-4294967295".to_string())?;
    let slider_before = get_optional_u64(tool_use, "slider_before")
        .map(u32::try_from)
        .transpose()
        .map_err(|_| "slider_before must fit within 0-4294967295".to_string())?;

    Ok(RhythmInput {
        kind: get_optional_str(tool_use, "type")
            .map(str::parse)
            .transpose()
            .map_err(|e: cadence_agent::Error| e.to_string())?,
        at: get_optional_str(tool_use, "at")
            .map(parse_time)
            .transpose()
            .map_err(|e| e.to_string())?,
        dotw,
        dotm,
        every_n_days,
        slider_before,
    })
}

#[async_trait::async_trait]
trait CadenceToolLogic<A: Agent>: Send + Sync {
    async fn compute(
        &self,
        client: &Anthropic,
        agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult>;
}

struct CadenceToolCallback<C> {
    inner: C,
}

impl<C> CadenceToolCallback<C> {
    fn new(inner: C) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl<A: Agent, C> ToolCallback<A> for CadenceToolCallback<C>
where
    C: CadenceToolLogic<A>,
{
    async fn compute_tool_result(
        &self,
        client: &Anthropic,
        agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        self.inner.compute(client, agent, tool_use).await
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_cadence_tool_result(intermediate)
    }
}

fn cadence_tool_callback<A: Agent, C: CadenceToolLogic<A> + 'static>(
    callback: C,
) -> Box<dyn ToolCallback<A>> {
    Box::new(CadenceToolCallback::new(callback))
}

///////////////////////////////////////// ListRhythmsTool //////////////////////////////////////////

struct ListRhythmsTool {
    document: SharedDocument,
}

impl ListRhythmsTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for ListRhythmsTool {
    fn name(&self) -> String {
        "list_rhythms".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(ListRhythmsCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "list_rhythms",
            "List all rhythms with optional pattern filter on description",
            json_schema(
                &[(
                    "pattern",
                    "Case-insensitive substring to filter rhythm descriptions",
                    serde_json::json!({"type": "string"}),
                )],
                &[],
            ),
        )
    }
}

struct ListRhythmsCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for ListRhythmsCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        let rhythms = list_rhythms(&doc, get_optional_str(tool_use, "pattern"), None);

        if rhythms.is_empty() {
            return tool_ok(&tool_use.id, "No rhythms found.");
        }

        let mut out = String::from("id | description | type | schedule\n");
        out.push_str("----|-------------|------|----------\n");
        for rhythm in rhythms {
            out.push_str(&format!(
                "{} | {} | {} | {}\n",
                rhythm.id,
                rhythm.description,
                rhythm_kind_name(rhythm.rhythm),
                rhythm.rhythm,
            ));
        }
        tool_ok(&tool_use.id, out)
    }
}

/////////////////////////////////////////// GetRhythmTool //////////////////////////////////////////

struct GetRhythmTool {
    document: SharedDocument,
}

impl GetRhythmTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for GetRhythmTool {
    fn name(&self) -> String {
        "get_rhythm".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(GetRhythmCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "get_rhythm",
            "Get full details of a specific rhythm by ID",
            json_schema(
                &[(
                    "id",
                    "Rhythm ID (e.g. rhythm:UUID)",
                    serde_json::json!({"type": "string"}),
                )],
                &["id"],
            ),
        )
    }
}

struct GetRhythmCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for GetRhythmCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let (id, rhythm_id) = match get_required_rhythm_id(tool_use, "id") {
            Ok(rhythm) => rhythm,
            Err(e) => return tool_err(&tool_use.id, e),
        };

        let doc = self.document.lock().unwrap();
        match doc.find_rhythm(rhythm_id) {
            Some(rhythm) => tool_ok(&tool_use.id, format_rhythm(rhythm)),
            None => tool_err(&tool_use.id, format!("Rhythm not found: {id}")),
        }
    }
}

///////////////////////////////////////////// TodayTool ////////////////////////////////////////////

struct TodayTool {
    document: SharedDocument,
}

impl TodayTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for TodayTool {
    fn name(&self) -> String {
        "today".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(TodayCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "today",
            "Show today's scheduled items, distinguishing regular items from stretch goals",
            json_schema(&[], &[]),
        )
    }
}

struct TodayCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for TodayCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        let user_tz = match document_timezone(&doc) {
            Ok(tz) => tz,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };
        let items = match today_items(&doc) {
            Ok(items) => items,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };

        tool_ok(
            &tool_use.id,
            render_today_items(
                &items,
                &user_tz,
                TodayRenderOptions {
                    include_regular_heading: true,
                },
            ),
        )
    }
}

/////////////////////////////////////////// ScheduleTool ///////////////////////////////////////////

struct ScheduleTool {
    document: SharedDocument,
}

impl ScheduleTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for ScheduleTool {
    fn name(&self) -> String {
        "schedule".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(ScheduleCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "schedule",
            "Show schedule for a date range",
            json_schema(
                &[
                    (
                        "start",
                        "Start date in YYYY-MM-DD format",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "days",
                        "Number of days to show",
                        serde_json::json!({"type": "integer", "minimum": 1, "maximum": 90}),
                    ),
                ],
                &["start", "days"],
            ),
        )
    }
}

struct ScheduleCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for ScheduleCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let start_str = match get_required_str(tool_use, "start") {
            Ok(s) => s,
            Err(e) => return tool_err(&tool_use.id, e),
        };
        let days = match get_required_u64(tool_use, "days") {
            Ok(d) => d as u32,
            Err(e) => return tool_err(&tool_use.id, e),
        };

        let start_date = match parse_date(start_str) {
            Ok(d) => d,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };

        let doc = self.document.lock().unwrap();
        let user_tz = match document_timezone(&doc) {
            Ok(tz) => tz,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };
        let items = match schedule_items(&doc, start_date, days) {
            Ok(items) => items,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };

        tool_ok(
            &tool_use.id,
            render_schedule_items(&items, &user_tz, "No scheduled items in this range."),
        )
    }
}

////////////////////////////////////////// DelinquentTool //////////////////////////////////////////

struct DelinquentTool {
    document: SharedDocument,
}

impl DelinquentTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for DelinquentTool {
    fn name(&self) -> String {
        "delinquent".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(DelinquentCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "delinquent",
            "Show rhythms that should have fired but have not been completed",
            json_schema(&[], &[]),
        )
    }
}

struct DelinquentCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for DelinquentCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        let items = match delinquent_items(&doc) {
            Ok(items) => items,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };
        tool_ok(&tool_use.id, render_delinquent_items(&items))
    }
}

///////////////////////////////////////// ConvergenceTool //////////////////////////////////////////

struct ConvergenceTool {
    document: SharedDocument,
}

impl ConvergenceTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for ConvergenceTool {
    fn name(&self) -> String {
        "convergence".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(ConvergenceCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "convergence",
            "Show when all rhythms next fire",
            json_schema(&[], &[]),
        )
    }
}

struct ConvergenceCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for ConvergenceCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        let user_tz = match document_timezone(&doc) {
            Ok(tz) => tz,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };

        let result = match convergence_response(&doc) {
            Ok(result) => result,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };

        let datetime = match result.datetime {
            Some(datetime) => datetime,
            None => {
                return tool_ok(&tool_use.id, "No rhythms configured.");
            }
        };
        let local_time = datetime.with_timezone(&user_tz);
        tool_ok(
            &tool_use.id,
            format!(
                "Next convergence: {}",
                local_time.format("%Y-%m-%d %H:%M:%S")
            ),
        )
    }
}

////////////////////////////////////////// ShowSpoonsTool //////////////////////////////////////////

struct ShowSpoonsTool {
    document: SharedDocument,
}

impl ShowSpoonsTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for ShowSpoonsTool {
    fn name(&self) -> String {
        "show_spoons".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(ShowSpoonsCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "show_spoons",
            "Show current spoons settings for all configured dates",
            json_schema(&[], &[]),
        )
    }
}

struct ShowSpoonsCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for ShowSpoonsCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        if doc.spoons.is_empty() {
            return tool_ok(&tool_use.id, "No spoons settings configured.");
        }

        let mut out = String::from("date | spoons\n-----|-------\n");
        for (date, value) in &doc.spoons {
            out.push_str(&format!("{date} | {value}\n"));
        }
        tool_ok(&tool_use.id, out)
    }
}

/////////////////////////////////////////// AddRhythmTool //////////////////////////////////////////

struct AddRhythmTool {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

impl AddRhythmTool {
    fn new(
        document: SharedDocument,
        file_path: SharedFilePath,
        schedule_changed: Option<Arc<Notify>>,
    ) -> Self {
        Self {
            document,
            file_path,
            schedule_changed,
        }
    }
}

impl<A: Agent> Tool<A> for AddRhythmTool {
    fn name(&self) -> String {
        "add_rhythm".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(AddRhythmCallback {
            document: Arc::clone(&self.document),
            file_path: self.file_path.clone(),
            schedule_changed: self.schedule_changed.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "add_rhythm",
            "Add a new rhythm",
            json_schema(
                &[
                    (
                        "type",
                        "Rhythm type: daily, weekly, monthly, or every_n_days",
                        serde_json::json!({"type": "string", "enum": ["daily", "weekly", "monthly", "every_n_days"]}),
                    ),
                    (
                        "at",
                        "Time in HH:MM:SS format",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "description",
                        "Description of the rhythm",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "dotw",
                        "Day of week (0=Monday, 6=Sunday) for weekly rhythms",
                        serde_json::json!({"type": "integer", "minimum": 0, "maximum": 6}),
                    ),
                    (
                        "dotm",
                        "Day of month (0-30, 0-based) for monthly rhythms",
                        serde_json::json!({"type": "integer", "minimum": 0, "maximum": 30}),
                    ),
                    (
                        "n",
                        "Number of days for every_n_days rhythms",
                        serde_json::json!({"type": "integer", "minimum": 1}),
                    ),
                    (
                        "slider_before",
                        "Days the rhythm can slide earlier (default 0)",
                        serde_json::json!({"type": "integer", "minimum": 0}),
                    ),
                ],
                &["type", "at", "description"],
            ),
        )
    }
}

struct AddRhythmCallback {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for AddRhythmCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let description = match get_required_str(tool_use, "description") {
            Ok(d) => d.to_string(),
            Err(e) => return tool_err(&tool_use.id, e),
        };
        let rhythm_input = match parse_rhythm_input(tool_use) {
            Ok(input) => input,
            Err(e) => return tool_err(&tool_use.id, e),
        };
        let rhythm = match build_rhythm(rhythm_input, None) {
            Ok(r) => r,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };

        let mut doc = self.document.lock().unwrap();
        let id = match add_rhythm(&mut doc, rhythm, description.clone(), chrono::Utc::now()) {
            Ok(id) => id,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };
        if let Err(err) =
            save_current_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Created rhythm")
        {
            return err;
        }
        notify_schedule_changed(&self.schedule_changed);

        tool_ok(
            &tool_use.id,
            format!("Rhythm created: {description} (ID: {id})"),
        )
    }
}

///////////////////////////////////////// UpdateRhythmTool /////////////////////////////////////////

struct UpdateRhythmTool {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

impl UpdateRhythmTool {
    fn new(
        document: SharedDocument,
        file_path: SharedFilePath,
        schedule_changed: Option<Arc<Notify>>,
    ) -> Self {
        Self {
            document,
            file_path,
            schedule_changed,
        }
    }
}

impl<A: Agent> Tool<A> for UpdateRhythmTool {
    fn name(&self) -> String {
        "update_rhythm".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(UpdateRhythmCallback {
            document: Arc::clone(&self.document),
            file_path: self.file_path.clone(),
            schedule_changed: self.schedule_changed.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "update_rhythm",
            "Update an existing rhythm's type, time, description, or slider",
            json_schema(
                &[
                    (
                        "id",
                        "Rhythm ID to update",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "type",
                        "New rhythm type: daily, weekly, monthly, or every_n_days",
                        serde_json::json!({"type": "string", "enum": ["daily", "weekly", "monthly", "every_n_days"]}),
                    ),
                    (
                        "at",
                        "New time in HH:MM:SS format",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "description",
                        "New description",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "dotw",
                        "Day of week (0=Monday, 6=Sunday) for weekly",
                        serde_json::json!({"type": "integer", "minimum": 0, "maximum": 6}),
                    ),
                    (
                        "dotm",
                        "Day of month (0-30) for monthly",
                        serde_json::json!({"type": "integer", "minimum": 0, "maximum": 30}),
                    ),
                    (
                        "n",
                        "Number of days for every_n_days",
                        serde_json::json!({"type": "integer", "minimum": 1}),
                    ),
                    (
                        "slider_before",
                        "Days the rhythm can slide earlier",
                        serde_json::json!({"type": "integer", "minimum": 0}),
                    ),
                ],
                &["id"],
            ),
        )
    }
}

struct UpdateRhythmCallback {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for UpdateRhythmCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let (id_str, rhythm_id) = match get_required_rhythm_id(tool_use, "id") {
            Ok(rhythm) => rhythm,
            Err(e) => return tool_err(&tool_use.id, e),
        };

        let mut doc = self.document.lock().unwrap();
        let current = match doc.find_rhythm(rhythm_id) {
            Some(rhythm) => rhythm.rhythm,
            None => return tool_err(&tool_use.id, format!("Rhythm not found: {id_str}")),
        };
        let rhythm_input = match parse_rhythm_input(tool_use) {
            Ok(input) => input,
            Err(e) => return tool_err(&tool_use.id, e),
        };
        let rhythm = match build_rhythm(rhythm_input, Some(current)) {
            Ok(rhythm) => rhythm,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };
        let description = get_optional_str(tool_use, "description").map(ToString::to_string);

        if let Err(e) = update_rhythm(
            &mut doc,
            rhythm_id,
            RhythmUpdate {
                rhythm: Some(rhythm),
                modified_at: chrono::Utc::now(),
            },
            description,
        ) {
            return tool_err(&tool_use.id, e.to_string());
        }

        if let Err(err) =
            save_current_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Updated rhythm")
        {
            return err;
        }
        notify_schedule_changed(&self.schedule_changed);

        tool_ok(&tool_use.id, format!("Rhythm {id_str} updated"))
    }
}

///////////////////////////////////////// DeleteRhythmTool /////////////////////////////////////////

struct DeleteRhythmTool {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

impl DeleteRhythmTool {
    fn new(
        document: SharedDocument,
        file_path: SharedFilePath,
        schedule_changed: Option<Arc<Notify>>,
    ) -> Self {
        Self {
            document,
            file_path,
            schedule_changed,
        }
    }
}

impl<A: Agent> Tool<A> for DeleteRhythmTool {
    fn name(&self) -> String {
        "delete_rhythm".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(DeleteRhythmCallback {
            document: Arc::clone(&self.document),
            file_path: self.file_path.clone(),
            schedule_changed: self.schedule_changed.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "delete_rhythm",
            "Delete a rhythm by ID",
            json_schema(
                &[(
                    "id",
                    "Rhythm ID to delete",
                    serde_json::json!({"type": "string"}),
                )],
                &["id"],
            ),
        )
    }
}

struct DeleteRhythmCallback {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for DeleteRhythmCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let (id_str, rhythm_id) = match get_required_rhythm_id(tool_use, "id") {
            Ok(rhythm) => rhythm,
            Err(e) => return tool_err(&tool_use.id, e),
        };

        let mut doc = self.document.lock().unwrap();
        if let Err(e) = delete_rhythm(&mut doc, rhythm_id) {
            return tool_err(&tool_use.id, e.to_string());
        }

        if let Err(err) =
            save_current_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Deleted rhythm")
        {
            return err;
        }
        notify_schedule_changed(&self.schedule_changed);

        tool_ok(&tool_use.id, format!("Rhythm {id_str} deleted"))
    }
}

/////////////////////////////////////////// MarkDoneTool ///////////////////////////////////////////

struct MarkDoneTool {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

impl MarkDoneTool {
    fn new(
        document: SharedDocument,
        file_path: SharedFilePath,
        schedule_changed: Option<Arc<Notify>>,
    ) -> Self {
        Self {
            document,
            file_path,
            schedule_changed,
        }
    }
}

impl<A: Agent> Tool<A> for MarkDoneTool {
    fn name(&self) -> String {
        "mark_done".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(MarkDoneCallback {
            document: Arc::clone(&self.document),
            file_path: self.file_path.clone(),
            schedule_changed: self.schedule_changed.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "mark_done",
            "Mark one or more rhythms as done",
            json_schema(
                &[(
                    "ids",
                    "Array of rhythm IDs to mark as done",
                    serde_json::json!({"type": "array", "items": {"type": "string"}}),
                )],
                &["ids"],
            ),
        )
    }
}

struct MarkDoneCallback {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for MarkDoneCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let ids = match tool_use
            .input
            .get("ids")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "Missing or invalid 'ids'".to_string())
        {
            Ok(ids) => ids.clone(),
            Err(e) => return tool_err(&tool_use.id, e),
        };

        let mut doc = self.document.lock().unwrap();
        let mut results = Vec::new();
        for id_val in &ids {
            let id_str = match id_val.as_str() {
                Some(s) => s,
                None => {
                    results.push(format!("Invalid ID value: {id_val}"));
                    continue;
                }
            };
            let rhythm_id = match parse_rhythm_id(id_str) {
                Ok(id) => id,
                Err(e) => {
                    results.push(e.to_string());
                    continue;
                }
            };
            match mark_rhythm_done(&mut doc, rhythm_id) {
                Ok(()) => results.push(format!("{id_str}: done")),
                Err(e) => results.push(format!("{id_str}: error - {}", e)),
            }
        }

        if let Err(err) =
            save_current_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Marked done")
        {
            return err;
        }
        notify_schedule_changed(&self.schedule_changed);

        tool_ok(&tool_use.id, results.join("\n"))
    }
}

///////////////////////////////////////// DeferRhythmTool //////////////////////////////////////////

struct DeferRhythmTool {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

impl DeferRhythmTool {
    fn new(
        document: SharedDocument,
        file_path: SharedFilePath,
        schedule_changed: Option<Arc<Notify>>,
    ) -> Self {
        Self {
            document,
            file_path,
            schedule_changed,
        }
    }
}

impl<A: Agent> Tool<A> for DeferRhythmTool {
    fn name(&self) -> String {
        "defer_rhythm".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(DeferRhythmCallback {
            document: Arc::clone(&self.document),
            file_path: self.file_path.clone(),
            schedule_changed: self.schedule_changed.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "defer_rhythm",
            "Defer a rhythm to its next occurrence",
            json_schema(
                &[(
                    "id",
                    "Rhythm ID to defer",
                    serde_json::json!({"type": "string"}),
                )],
                &["id"],
            ),
        )
    }
}

struct DeferRhythmCallback {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for DeferRhythmCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let (id_str, rhythm_id) = match get_required_rhythm_id(tool_use, "id") {
            Ok(rhythm) => rhythm,
            Err(e) => return tool_err(&tool_use.id, e),
        };

        let mut doc = self.document.lock().unwrap();
        if let Err(e) = defer_rhythm(&mut doc, rhythm_id) {
            return tool_err(&tool_use.id, e.to_string());
        }
        if let Err(err) = save_current_document_or_tool_err(
            &tool_use.id,
            &self.file_path,
            &doc,
            "Deferred rhythm",
        ) {
            return err;
        }
        notify_schedule_changed(&self.schedule_changed);

        tool_ok(&tool_use.id, format!("Rhythm {id_str} deferred"))
    }
}

/////////////////////////////////////////// SetSpoonsTool //////////////////////////////////////////

struct SetSpoonsTool {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

impl SetSpoonsTool {
    fn new(
        document: SharedDocument,
        file_path: SharedFilePath,
        schedule_changed: Option<Arc<Notify>>,
    ) -> Self {
        Self {
            document,
            file_path,
            schedule_changed,
        }
    }
}

impl<A: Agent> Tool<A> for SetSpoonsTool {
    fn name(&self) -> String {
        "set_spoons".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(SetSpoonsCallback {
            document: Arc::clone(&self.document),
            file_path: self.file_path.clone(),
            schedule_changed: self.schedule_changed.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "set_spoons",
            "Set energy level (spoons) for a date. 0-10 scale, 5 is neutral.",
            json_schema(
                &[
                    (
                        "date",
                        "Date in YYYY-MM-DD format",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "value",
                        "Spoons value (0-10)",
                        serde_json::json!({"type": "integer", "minimum": 0, "maximum": 10}),
                    ),
                ],
                &["date", "value"],
            ),
        )
    }
}

struct SetSpoonsCallback {
    document: SharedDocument,
    file_path: SharedFilePath,
    schedule_changed: Option<Arc<Notify>>,
}

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for SetSpoonsCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let date_str = match get_required_str(tool_use, "date") {
            Ok(d) => d,
            Err(e) => return tool_err(&tool_use.id, e),
        };
        let value = match tool_use
            .input
            .get("value")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Missing or invalid 'value'".to_string())
        {
            Ok(v) => v as u8,
            Err(e) => return tool_err(&tool_use.id, e),
        };

        let date = match parse_date(date_str) {
            Ok(d) => d,
            Err(e) => return tool_err(&tool_use.id, e.to_string()),
        };

        let mut doc = self.document.lock().unwrap();
        if let Err(e) = set_spoons(&mut doc, date, value) {
            return tool_err(&tool_use.id, e.to_string());
        }

        if let Err(err) =
            save_current_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Set spoons")
        {
            return err;
        }
        notify_schedule_changed(&self.schedule_changed);

        tool_ok(&tool_use.id, format!("Spoons set to {value} for {date}"))
    }
}

/// Returns the Cadence agent state directory, creating it if necessary.
fn agent_state_dir() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rhythm");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn default_transport_state_path() -> PathBuf {
    agent_state_dir().join("cadence-agent-transport-state.json")
}

fn default_yaml_dir() -> PathBuf {
    default_file_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(agent_state_dir)
}

fn timestamped_user_message_content_at(input: &str, timestamp: chrono::DateTime<Tz>) -> String {
    format!(
        "Current timestamp: {} ({})\n\n{}",
        timestamp.to_rfc3339(),
        timestamp.timezone(),
        input
    )
}

fn timestamped_user_message_content(input: &str, document: &SharedDocument) -> String {
    let tz = {
        let doc = document.lock().unwrap();
        document_timezone(&doc).unwrap_or(Tz::UTC)
    };
    let now = chrono::Utc::now().with_timezone(&tz);
    timestamped_user_message_content_at(input, now)
}

fn stamp_latest_user_message(messages: &mut [MessageParam], document: &SharedDocument) {
    let Some(message) = messages
        .iter_mut()
        .rev()
        .find(|message| message.role == MessageRole::User)
    else {
        return;
    };
    let MessageParamContent::String(text) = &mut message.content else {
        return;
    };
    if text.starts_with("Current timestamp: ") {
        return;
    }
    let raw = std::mem::take(text);
    *text = timestamped_user_message_content(&raw, document);
}

fn parse_duration_seconds(raw: &str) -> Result<i64, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("duration must not be empty".to_string());
    }

    let split_at = raw
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(raw.len());
    let (number, unit) = raw.split_at(split_at);
    if number.is_empty() {
        return Err(format!("invalid duration: {raw}"));
    }
    let value = number
        .parse::<i64>()
        .map_err(|_| format!("invalid duration value: {raw}"))?;
    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        "d" | "day" | "days" => 24 * 60 * 60,
        other => return Err(format!("unsupported duration unit: {other}")),
    };
    value
        .checked_mul(multiplier)
        .ok_or_else(|| format!("duration is too large: {raw}"))
}

fn proactive_min_interval(raw: Option<&str>) -> Result<ChronoDuration, TelegramError> {
    let seconds = match raw {
        Some(raw) => parse_duration_seconds(raw).map_err(|err| {
            TelegramError::Internal(format!("invalid --proactive-min-interval: {err}"))
        })?,
        None => DEFAULT_PROACTIVE_MIN_INTERVAL_SECS,
    };
    Ok(ChronoDuration::seconds(seconds))
}

fn render_proactive_group(group: &cadence_agent::ProactiveDueGroup, user_tz: &Tz) -> Vec<String> {
    group
        .items
        .iter()
        .map(|item| {
            let local_time = item.datetime.with_timezone(user_tz);
            format!(
                "- {} - {} [{}]",
                local_time.format("%Y-%m-%d %H:%M:%S"),
                item.description,
                item.rhythm_id
            )
        })
        .collect()
}

fn build_proactive_check_in_prompt(
    document: &CadenceDocument,
    plan: &ProactiveCheckInPlan,
    now_utc: DateTime<Utc>,
) -> String {
    let user_tz = document_timezone(document).unwrap_or(Tz::UTC);
    let now = now_utc.with_timezone(&user_tz);
    let mut lines = vec![
        format!(
            "Current timestamp: {} ({})",
            now.to_rfc3339(),
            now.timezone()
        ),
        String::new(),
        "Proactive wakeup: the following scheduled items are due now:".to_string(),
    ];
    lines.extend(render_proactive_group(&plan.due_now, &user_tz));

    if !plan.backlog.is_empty() {
        lines.push(String::new());
        lines.push("Earlier unchecked scheduled groups:".to_string());
        for group in &plan.backlog {
            let due_at = group.due_at_utc.with_timezone(&user_tz);
            lines.push(format!("Due at {}:", due_at.format("%Y-%m-%d %H:%M:%S")));
            lines.extend(render_proactive_group(group, &user_tz));
        }
    }

    lines.push(String::new());
    lines.push(
        "Send the user a concise, natural check-in about these due items. \
         Do not mark anything done, defer rhythms, add rhythms, update rhythms, \
         delete rhythms, or set spoons in this proactive turn. Ask the user what \
         they want to do instead of assuming completion."
            .to_string(),
    );
    lines.join("\n")
}

fn chat_has_pending_outbox(state: &TransportState, chat: ChatId) -> bool {
    state
        .outbox
        .iter()
        .any(|record| record.chat == chat && record.status == OutboxStatus::Pending)
}

fn proactive_rate_limit_retry_at(
    document: &CadenceDocument,
    now: DateTime<Utc>,
    min_interval: ChronoDuration,
) -> Option<DateTime<Utc>> {
    let retry_at = document.last_proactive_check_in_utc? + min_interval;
    if retry_at <= now {
        return None;
    }
    let has_due_backlog = proactive_due_groups(document, now, PROACTIVE_SCAN_HORIZON_DAYS)
        .ok()?
        .iter()
        .any(|group| group.due_at_utc <= now);
    has_due_backlog.then_some(retry_at)
}

fn save_proactive_check_in_record(
    path: &Path,
    plan: &ProactiveCheckInPlan,
    checked_in_at_utc: DateTime<Utc>,
) -> Result<(), cadence_agent::Error> {
    let mut document = load_document(path)?;
    record_proactive_check_in_plan(&mut document, plan, checked_in_at_utc);
    save_document(path, &document)
}

#[derive(Clone, Copy)]
enum ProactivePass {
    Startup,
    Scheduled(DateTime<Utc>),
}

#[allow(clippy::too_many_arguments)]
async fn run_model_proactive_check_in<T>(
    agent_template: &SharedCadenceAgent,
    transport: &T,
    client: &Anthropic,
    store: &dyn StateStore,
    state: &Arc<AsyncMutex<TransportState>>,
    budget: &Arc<Budget>,
    chat: ChatId,
    path: &Path,
    document: &CadenceDocument,
    plan: &ProactiveCheckInPlan,
    history_message_threshold: usize,
) -> Result<(), TelegramError>
where
    T: ChatTransport + Sync + ?Sized,
{
    let checked_in_at_utc = Utc::now();
    let prompt = build_proactive_check_in_prompt(document, plan, checked_in_at_utc);
    let document_context: SharedDocument = Arc::new(Mutex::new(document.clone()));
    let file_path_context: SharedFilePath = Arc::new(Mutex::new(path.to_path_buf()));
    let mut agent = agent_template
        .with_turn_source("proactive")
        .with_document_context(document_context, file_path_context);

    run_synthetic_user_turn(
        &mut agent,
        transport,
        client,
        store,
        state,
        budget,
        chat,
        prompt,
        SyntheticTurnConfig {
            max_history_messages: Some(history_message_threshold),
            reset_on_context_limit: true,
            use_outbox: true,
            source_label: Some("cadence-proactive".to_string()),
        },
    )
    .await?;

    save_proactive_check_in_record(path, plan, checked_in_at_utc).map_err(|err| {
        TelegramError::Internal(format!(
            "sent proactive check-in for chat {chat} but failed to record it in {}: {err}",
            path.display()
        ))
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_proactive_pass<T>(
    pass: ProactivePass,
    agent: &SharedCadenceAgent,
    transport: &T,
    client: &Anthropic,
    store: &Arc<dyn StateStore>,
    state: &Arc<AsyncMutex<TransportState>>,
    budget: &Arc<Budget>,
    yaml_dir: &Path,
    min_interval: ChronoDuration,
    history_message_threshold: usize,
    interrupted: &Arc<AtomicBool>,
) -> Option<DateTime<Utc>>
where
    T: ChatTransport + Sync + ?Sized,
{
    let chats = {
        let state = state.lock().await;
        known_chats(&state)
    };
    let mut next_due_at = None;

    for chat in chats {
        if interrupted.load(Ordering::SeqCst) {
            break;
        }

        let has_pending = {
            let state = state.lock().await;
            chat_has_pending_outbox(&state, chat)
        };
        if has_pending {
            eprintln!(
                "[cadence-agent] skipping proactive check-in for chat {chat}; pending outbox exists"
            );
            continue;
        }

        let path = user_yaml_path(yaml_dir, chat.0);
        if !path.is_file() {
            continue;
        }

        let document = match load_document(&path) {
            Ok(document) => document,
            Err(err) => {
                eprintln!(
                    "[cadence-agent] failed to load proactive document {}: {err}",
                    path.display()
                );
                continue;
            }
        };
        let now = Utc::now();
        let plan = match pass {
            ProactivePass::Startup => plan_startup_proactive_check_in(
                &document,
                now,
                min_interval,
                PROACTIVE_SCAN_HORIZON_DAYS,
            ),
            ProactivePass::Scheduled(scheduled_due_at_utc) => plan_scheduled_proactive_check_in(
                &document,
                scheduled_due_at_utc,
                now,
                min_interval,
                PROACTIVE_SCAN_HORIZON_DAYS,
            ),
        };

        match plan {
            Ok(Some(plan)) => {
                eprintln!(
                    "[cadence-agent] proactive check-in chat={chat} due_at={}",
                    plan.due_now.due_at_utc
                );
                if let Err(err) = run_model_proactive_check_in(
                    agent,
                    transport,
                    client,
                    store.as_ref(),
                    state,
                    budget,
                    chat,
                    &path,
                    &document,
                    &plan,
                    history_message_threshold,
                )
                .await
                {
                    eprintln!("[cadence-agent] proactive check-in failed for chat {chat}: {err}");
                }
            }
            Ok(None) => {
                if let Some(retry_at) = proactive_rate_limit_retry_at(&document, now, min_interval)
                {
                    next_due_at = Some(
                        next_due_at
                            .map_or(retry_at, |current: DateTime<Utc>| current.min(retry_at)),
                    );
                }
            }
            Err(err) => {
                eprintln!(
                    "[cadence-agent] failed to plan proactive check-in for {}: {err}",
                    path.display()
                );
            }
        }

        let next_document = load_document(&path).unwrap_or(document);
        let now = Utc::now();
        match next_proactive_due_at_after(&next_document, now, PROACTIVE_SCAN_HORIZON_DAYS) {
            Ok(Some(due_at)) => {
                next_due_at =
                    Some(next_due_at.map_or(due_at, |current: DateTime<Utc>| current.min(due_at)));
            }
            Ok(None) => {}
            Err(err) => {
                eprintln!(
                    "[cadence-agent] failed to find next proactive due time for {}: {err}",
                    path.display()
                );
            }
        }
    }

    next_due_at
}

#[allow(clippy::too_many_arguments)]
async fn run_proactive_scheduler<T>(
    agent: SharedCadenceAgent,
    transport: T,
    client: Anthropic,
    store: Arc<dyn StateStore>,
    state: Arc<AsyncMutex<TransportState>>,
    budget: Arc<Budget>,
    yaml_dir: PathBuf,
    min_interval: ChronoDuration,
    history_message_threshold: usize,
    schedule_changed: Arc<Notify>,
    interrupted: Arc<AtomicBool>,
) where
    T: ChatTransport + Sync,
{
    let mut pass = ProactivePass::Startup;
    loop {
        if interrupted.load(Ordering::SeqCst) {
            break;
        }

        let next_due_at = run_proactive_pass(
            pass,
            &agent,
            &transport,
            &client,
            &store,
            &state,
            &budget,
            &yaml_dir,
            min_interval,
            history_message_threshold,
            &interrupted,
        )
        .await;

        if interrupted.load(Ordering::SeqCst) {
            break;
        }

        let idle_rescan = StdDuration::from_secs(PROACTIVE_IDLE_RESCAN_SECS);
        let now = Utc::now();
        let (sleep_for, next_pass) = match next_due_at {
            Some(due_at) => {
                let until_due = due_at
                    .signed_duration_since(now)
                    .to_std()
                    .unwrap_or(StdDuration::ZERO);
                if until_due <= idle_rescan {
                    (until_due, ProactivePass::Scheduled(due_at))
                } else {
                    (idle_rescan, ProactivePass::Startup)
                }
            }
            None => (idle_rescan, ProactivePass::Startup),
        };

        tokio::select! {
            _ = tokio::time::sleep(sleep_for) => {
                pass = next_pass;
            }
            _ = schedule_changed.notified() => {
                pass = ProactivePass::Startup;
            }
        }
    }
}

fn is_human_user_message(message: &MessageParam) -> bool {
    message.role == MessageRole::User && matches!(message.content, MessageParamContent::String(_))
}

fn latest_human_user_message_index(messages: &[MessageParam]) -> Option<usize> {
    messages.iter().rposition(is_human_user_message)
}

fn split_timestamped_user_text(input: &str) -> (Option<String>, Option<String>, String) {
    let Some(rest) = input.strip_prefix("Current timestamp: ") else {
        return (None, None, input.to_string());
    };
    let Some((header, text)) = rest.split_once("\n\n") else {
        return (None, None, input.to_string());
    };
    let (timestamp, timezone) = if let Some((timestamp, tz)) = header.split_once(" (") {
        (
            Some(timestamp.to_string()),
            tz.strip_suffix(')').map(str::to_string),
        )
    } else {
        (Some(header.to_string()), None)
    };
    (timestamp, timezone, text.to_string())
}

fn request_log_path(yaml_path: &Path) -> PathBuf {
    yaml_path.with_extension("jsonl")
}

fn chat_id_from_yaml_path(yaml_path: &Path) -> Option<i64> {
    yaml_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.parse::<i64>().ok())
}

fn content_items_to_text(items: &[Content]) -> String {
    items
        .iter()
        .filter_map(|item| match item {
            Content::Text(text) => Some(text.text.as_str()),
            Content::Image(_) => Some("[image]"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_result_summary(block: &ToolResultBlock) -> Value {
    let text = match &block.content {
        Some(ToolResultBlockContent::String(text)) => text.clone(),
        Some(ToolResultBlockContent::Array(items)) => content_items_to_text(items),
        None => String::new(),
    };
    json!({
        "is_error": block.is_error.unwrap_or(false),
        "content": serde_json::to_value(&block.content).unwrap_or(Value::Null),
        "content_text": text,
    })
}

fn attach_tool_result(tools: &mut Vec<Value>, tool_use_id: &str, result: Value) {
    for tool in tools.iter_mut().rev() {
        if tool
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id == tool_use_id)
        {
            if let Some(obj) = tool.as_object_mut() {
                obj.insert("result".to_string(), result);
            }
            return;
        }
    }
    tools.push(json!({
        "id": tool_use_id,
        "result": result,
    }));
}

#[allow(clippy::too_many_arguments)]
fn build_request_log_entry(
    source: &str,
    chat_id: Option<i64>,
    yaml_path: &Path,
    turn_id: Option<&str>,
    model: &Model,
    imported: bool,
    import_source: Option<&Path>,
    messages: &[MessageParam],
) -> Option<Value> {
    let user_message = messages
        .iter()
        .find(|message| is_human_user_message(message))?;
    let MessageParamContent::String(raw_user_text) = &user_message.content else {
        return None;
    };
    let (timestamp, timezone, user_text) = split_timestamped_user_text(raw_user_text);
    let mut assistant_text = Vec::new();
    let mut tools = Vec::new();
    let mut skipped_blocks = BTreeSet::new();

    for message in messages {
        match &message.content {
            MessageParamContent::String(text) if message.role == MessageRole::Assistant => {
                assistant_text.push(text.clone());
            }
            MessageParamContent::String(_) => {}
            MessageParamContent::Array(blocks) => {
                for block in blocks {
                    match block {
                        ContentBlock::Text(text) if message.role == MessageRole::Assistant => {
                            assistant_text.push(text.text.clone());
                        }
                        ContentBlock::ToolUse(tool_use) => {
                            tools.push(json!({
                                "id": tool_use.id,
                                "name": tool_use.name,
                                "input": tool_use.input,
                            }));
                        }
                        ContentBlock::ToolResult(tool_result) => {
                            attach_tool_result(
                                &mut tools,
                                &tool_result.tool_use_id,
                                tool_result_summary(tool_result),
                            );
                        }
                        ContentBlock::Text(_) => {
                            skipped_blocks.insert("non_assistant_text");
                        }
                        ContentBlock::Thinking(_) => {
                            skipped_blocks.insert("thinking");
                        }
                        ContentBlock::RedactedThinking(_) => {
                            skipped_blocks.insert("redacted_thinking");
                        }
                        ContentBlock::Image(_) => {
                            skipped_blocks.insert("image");
                        }
                        ContentBlock::ServerToolUse(_) => {
                            skipped_blocks.insert("server_tool_use");
                        }
                        ContentBlock::WebSearchToolResult(_) => {
                            skipped_blocks.insert("web_search_tool_result");
                        }
                        ContentBlock::Document(_) => {
                            skipped_blocks.insert("document");
                        }
                    }
                }
            }
        }
    }

    let generated_turn_id;
    let turn_id = match turn_id {
        Some(turn_id) => turn_id,
        None => {
            generated_turn_id = format!("live:{}", chrono::Utc::now().timestamp_micros());
            &generated_turn_id
        }
    };
    let messages_value = serde_json::to_value(messages).unwrap_or(Value::Null);

    let mut entry = json!({
        "schema": REQUEST_LOG_SCHEMA,
        "turn_id": turn_id,
        "source": source,
        "imported": imported,
        "recorded_at": chrono::Utc::now().to_rfc3339(),
        "timestamp": timestamp,
        "timezone": timezone,
        "chat_id": chat_id,
        "yaml_path": yaml_path.to_string_lossy(),
        "model": model.to_string(),
        "message_count": messages.len(),
        "messages": messages_value,
        "user_text": user_text,
        "assistant_text": assistant_text.join("\n"),
        "tools": tools,
        "skipped_blocks": skipped_blocks.into_iter().collect::<Vec<_>>(),
    });
    if let Some(import_source) = import_source
        && let Some(obj) = entry.as_object_mut()
    {
        obj.insert(
            "import_source".to_string(),
            Value::String(import_source.to_string_lossy().to_string()),
        );
    }
    Some(entry)
}

fn append_request_log_entry(yaml_path: &Path, entry: &Value) -> io::Result<()> {
    let log_path = request_log_path(yaml_path);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let line = serde_json::to_string(entry)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    writeln!(file, "{line}")
}

fn existing_request_log_turn_ids(log_path: &Path) -> io::Result<BTreeSet<String>> {
    let text = match fs::read_to_string(log_path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(err) => return Err(err),
    };
    let mut ids = BTreeSet::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("schema").and_then(Value::as_str) == Some(REQUEST_LOG_SCHEMA)
            && let Some(turn_id) = value.get("turn_id").and_then(Value::as_str)
        {
            ids.insert(turn_id.to_string());
        }
    }
    Ok(ids)
}

fn import_request_logs_from_state(
    state_path: &Path,
    yaml_dir: &Path,
    model: &Model,
) -> Result<usize, Box<dyn std::error::Error>> {
    let bytes = fs::read(state_path)?;
    let state: TransportState = serde_json::from_slice(&bytes)?;
    let mut imported = 0usize;

    for (chat_id, messages) in &state.conversations {
        let yaml_path = user_yaml_path(yaml_dir, *chat_id);
        let log_path = request_log_path(&yaml_path);
        let mut existing = existing_request_log_turn_ids(&log_path)?;
        let mut turn_start = None;
        let mut turn_index = 0usize;

        for index in 0..=messages.len() {
            let starts_next_turn =
                index < messages.len() && is_human_user_message(&messages[index]);
            if (starts_next_turn || index == messages.len())
                && let Some(start) = turn_start
            {
                let turn_id = format!("state:{chat_id}:{turn_index}");
                if !existing.contains(&turn_id)
                    && let Some(entry) = build_request_log_entry(
                        "import",
                        Some(*chat_id),
                        &yaml_path,
                        Some(&turn_id),
                        model,
                        true,
                        Some(state_path),
                        &messages[start..index],
                    )
                {
                    append_request_log_entry(&yaml_path, &entry)?;
                    existing.insert(turn_id);
                    imported += 1;
                }
                turn_index += 1;
                turn_start = None;
            }
            if starts_next_turn {
                turn_start = Some(index);
            }
        }
    }

    Ok(imported)
}

fn user_yaml_path(yaml_dir: &Path, user_id: i64) -> PathBuf {
    yaml_dir.join(format!("{user_id}.yaml"))
}

fn allowed_user_ids_from_yaml_dir(yaml_dir: &Path) -> Result<Vec<i64>, TelegramError> {
    let mut ids = BTreeSet::new();
    for entry in fs::read_dir(yaml_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }
        if let Some(id) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<i64>().ok())
        {
            ids.insert(id);
        }
    }
    if ids.is_empty() {
        // `claudius-telegram` treats an empty allow-list as allow-all.
        ids.insert(i64::MIN);
    }
    Ok(ids.into_iter().collect())
}

fn user_yaml_pre_filter(
    yaml_dir: PathBuf,
    document: SharedDocument,
    file_path: SharedFilePath,
    fallback_user_id: Option<i64>,
    require_existing: bool,
) -> Box<dyn Fn(&Inbound) -> PreFilter + Send> {
    Box::new(move |inbound| {
        let Some(user_id) = inbound.from_user_id.or(fallback_user_id) else {
            return PreFilter::handled_text("No Telegram user id was attached to this message.");
        };
        let path = user_yaml_path(&yaml_dir, user_id);
        if require_existing && !path.is_file() {
            return PreFilter::handled_text("");
        }
        match load_document(&path) {
            Ok(loaded) => {
                *document.lock().unwrap() = loaded;
                *file_path.lock().unwrap() = path;
                PreFilter::PassToAgent
            }
            Err(err) => PreFilter::handled_text(format!(
                "Failed to load cadence file for user {user_id}: {err}"
            )),
        }
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (options, _free) = CliOptions::from_command_line_relaxed(
        "Usage: cadence-agent [--yaml DIR] [--model MODEL] [--import-request-log PATH] [--mutation-cap N] [--history-message-threshold N] [--proactive-min-interval DURATION] [--stdin] [--state PATH]",
    );

    let history_message_threshold = options
        .history_message_threshold
        .unwrap_or(DEFAULT_HISTORY_MESSAGE_THRESHOLD);
    let yaml_dir = options
        .yaml
        .map(PathBuf::from)
        .unwrap_or_else(default_yaml_dir);
    std::fs::create_dir_all(&yaml_dir)?;

    let model_name = options.model.as_deref().unwrap_or(DEFAULT_MODEL);
    let model = model_name
        .parse::<Model>()
        .unwrap_or_else(|_| Model::Custom(model_name.to_string()));

    if let Some(import_request_log) = options.import_request_log.as_deref() {
        let imported =
            import_request_logs_from_state(Path::new(import_request_log), &yaml_dir, &model)?;
        println!("imported {imported} request log entries");
        return Ok(());
    }

    let initial_file_path = yaml_dir.join("0.yaml");
    let document: SharedDocument = Arc::new(Mutex::new(CadenceDocument::new("UTC")));
    let file_path: SharedFilePath = Arc::new(Mutex::new(initial_file_path));
    let schedule_changed = Arc::new(Notify::new());

    let client = Anthropic::new(None)?;

    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_clone = interrupted.clone();
    ctrlc::set_handler(move || {
        interrupted_clone.store(true, Ordering::Relaxed);
    })?;

    let mutation_counter = Arc::new(MutationCounter::new(
        options.mutation_cap.unwrap_or(DEFAULT_MUTATION_CAP),
    ));

    let config = ChatConfig::new()
        .with_model(model.clone())
        .with_max_tokens(1536)
        .with_thinking_budget(Some(1024))
        .with_caching(true);

    let agent = CadenceAgent::new(
        Arc::clone(&document),
        Arc::clone(&mutation_counter),
        Arc::clone(&file_path),
        config,
        Some(Arc::clone(&schedule_changed)),
    );
    let agent = SharedCadenceAgent::new(agent);

    let state_path = options
        .state
        .map(PathBuf::from)
        .unwrap_or_else(default_transport_state_path);
    let store: Arc<dyn StateStore> = Arc::new(FileStateStore::new(state_path.clone()));
    let loaded: TransportState = store.load().await?;
    let state = Arc::new(AsyncMutex::new(loaded));
    let budget = Arc::new(Budget::new_flat_rate(u64::MAX, 1));

    eprintln!("[cadence-agent] yaml dir: {}", yaml_dir.display());
    eprintln!("[cadence-agent] transport state: {}", state_path.display());
    eprintln!("[cadence-agent] model: {model}");
    eprintln!("[cadence-agent] history message threshold: {history_message_threshold}");

    if options.stdin {
        let transport = StdinTransport::new(Arc::clone(&interrupted));
        let mut loop_config = LoopConfig::new(store, state, budget, interrupted);
        loop_config.pre_filter = Some(user_yaml_pre_filter(
            yaml_dir,
            document,
            file_path,
            Some(0),
            false,
        ));
        loop_config.max_history_messages = Some(history_message_threshold);
        loop_config.reset_on_context_limit = true;
        run_agent_loop(agent, transport, client, loop_config).await?;
    } else {
        let token = options
            .token
            .clone()
            .or_else(|| env::var("TELEGRAM_BOT_TOKEN").ok())
            .ok_or_else(|| {
                TelegramError::Internal(
                    "no token: pass --token, set TELEGRAM_BOT_TOKEN, or use --stdin".to_string(),
                )
            })?;
        let mut transport =
            TelegramTransport::new(token.clone(), Arc::clone(&state), Arc::clone(&store))?;
        let allowed_user_ids = allowed_user_ids_from_yaml_dir(&yaml_dir)?;
        let allowed_count = allowed_user_ids
            .iter()
            .filter(|&&id| id != i64::MIN)
            .count();
        transport = transport.with_allowed_user_ids(allowed_user_ids);
        if let Some(secs) = options.poll_timeout {
            transport = transport.with_poll_timeout(secs, token.clone())?;
        }
        if options.delete_webhook_on_start {
            transport.delete_webhook().await?;
        }
        match transport.get_me().await {
            Ok(username) => eprintln!("[cadence-agent] connected as @{username}"),
            Err(err) => eprintln!("[cadence-agent] getMe failed: {err}"),
        }
        eprintln!("[cadence-agent] allowed users: {allowed_count}");

        let scheduler_handle = if options.disable_proactive_checkins {
            eprintln!("[cadence-agent] proactive check-ins disabled");
            None
        } else {
            let proactive_min_interval =
                proactive_min_interval(options.proactive_min_interval.as_deref())?;
            eprintln!(
                "[cadence-agent] proactive check-ins enabled; min interval: {}s",
                proactive_min_interval.num_seconds()
            );
            let proactive_transport =
                TelegramTransport::new(token.clone(), Arc::clone(&state), Arc::clone(&store))?;
            let proactive_client = Anthropic::new(None)?;
            Some(tokio::spawn(run_proactive_scheduler(
                agent.clone(),
                proactive_transport,
                proactive_client,
                Arc::clone(&store),
                Arc::clone(&state),
                Arc::clone(&budget),
                yaml_dir.clone(),
                proactive_min_interval,
                history_message_threshold,
                Arc::clone(&schedule_changed),
                Arc::clone(&interrupted),
            )))
        };

        let mut loop_config = LoopConfig::new(
            Arc::clone(&store),
            Arc::clone(&state),
            Arc::clone(&budget),
            Arc::clone(&interrupted),
        );
        loop_config.pre_filter = Some(user_yaml_pre_filter(
            yaml_dir, document, file_path, None, true,
        ));
        loop_config.max_history_messages = Some(history_message_threshold);
        loop_config.reset_on_context_limit = true;
        let loop_result = run_agent_loop(agent, transport, client, loop_config).await;
        if let Some(handle) = scheduler_handle {
            handle.abort();
        }
        loop_result?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamped_message_content_includes_timestamp_and_body() {
        let tz = Tz::UTC;
        let timestamp = chrono::TimeZone::with_ymd_and_hms(&tz, 2026, 5, 2, 12, 34, 56)
            .single()
            .expect("valid timestamp");
        let content = timestamped_user_message_content_at("review my rhythms", timestamp);

        assert!(content.contains("Current timestamp: 2026-05-02T12:34:56+00:00 (UTC)"));
        assert!(content.ends_with("\n\nreview my rhythms"));
    }

    #[test]
    fn system_prompt_ignores_missing_personality_file() {
        let path = std::env::temp_dir().join(format!(
            "cadence-agent-missing-personality-{}.yaml",
            std::process::id()
        ));

        let prompt = build_system_prompt_for_yaml_path(&path);

        assert_eq!(prompt, DEFAULT_SYSTEM_PROMPT);
    }

    #[test]
    fn system_prompt_appends_adjacent_personality_file() {
        let base =
            std::env::temp_dir().join(format!("cadence-agent-personality-{}", std::process::id()));
        let yaml_path = base.with_extension("yaml");
        let md_path = base.with_extension("md");
        fs::write(&md_path, "Use short, direct replies.\n").unwrap();

        let prompt = build_system_prompt_for_yaml_path(&yaml_path);

        assert!(prompt.starts_with(DEFAULT_SYSTEM_PROMPT));
        assert!(prompt.contains("<personality>\nUse short, direct replies.\n</personality>"));
        fs::remove_file(md_path).unwrap();
    }

    #[test]
    fn proactive_duration_parser_accepts_common_units() {
        assert_eq!(parse_duration_seconds("30"), Ok(30));
        assert_eq!(parse_duration_seconds("5m"), Ok(300));
        assert_eq!(parse_duration_seconds("2 hours"), Ok(7200));
        assert_eq!(parse_duration_seconds("1d"), Ok(86_400));
    }

    #[test]
    fn proactive_duration_parser_rejects_unknown_units() {
        assert!(parse_duration_seconds("").is_err());
        assert!(parse_duration_seconds("soon").is_err());
        assert!(parse_duration_seconds("1fortnight").is_err());
    }
}
