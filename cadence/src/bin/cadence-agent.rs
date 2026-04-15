//! Interactive rhythm management agent.
//!
//! This tool provides an interactive chat interface with an AI agent to help manage
//! recurring rhythms and habits.  The conversation operates on a YAML cadence document.
//!
//! Usage: `cadence-agent [--file PATH]`

use std::any::Any;
use std::io::BufRead;
use std::io::Write;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use arrrg::CommandLine;
use cadence_agent::{
    CadenceDocument, RhythmID, RhythmInput, RhythmUpdate, TodayRenderOptions, add_rhythm,
    build_rhythm, convergence_response, default_file_path, defer_rhythm, delete_rhythm,
    delinquent_items, document_timezone, format_rhythm, list_rhythms, load_document,
    mark_rhythm_done, parse_date, parse_rhythm_id, parse_time, render_delinquent_items,
    render_schedule_items, render_today_items, rhythm_kind_name, save_document, schedule_items,
    set_spoons, today_items, update_rhythm,
};
use chrono_tz::Tz;
use claudius::Agent;
use claudius::AgentStreamContext;
use claudius::Anthropic;
use claudius::Budget;
use claudius::CacheControlEphemeral;
use claudius::ContentBlock;
use claudius::IntermediateToolResult;
use claudius::Message;
use claudius::MessageParam;
use claudius::MessageParamContent;
use claudius::MessageRole;
use claudius::Model;
use claudius::PlainTextRenderer;
use claudius::Renderer;
use claudius::StopReason;
use claudius::SystemPrompt;
use claudius::TextBlock;
use claudius::Tool;
use claudius::ToolCallback;
use claudius::ToolParam;
use claudius::ToolResult;
use claudius::ToolResultBlock;
use claudius::ToolResultBlockContent;
use claudius::ToolUnionParam;
use claudius::ToolUseBlock;
use serde_json::Value;

const DEFAULT_SYSTEM_PROMPT: &str = include_str!("../../prompts/default-system.md");

////////////////////////////////////////////// Config //////////////////////////////////////////////

#[derive(arrrg_derive::CommandLine, Debug, Default, Eq, PartialEq)]
struct CliOptions {
    #[arrrg(optional, "YAML file path", "PATH")]
    file: Option<String>,
    #[arrrg(optional, "Override the default mutation cap per turn", "N")]
    mutation_cap: Option<usize>,
}

//////////////////////////////////////////// Shared State //////////////////////////////////////////

/// Shared cadence document state for the agent.
type SharedDocument = Arc<Mutex<CadenceDocument>>;

/// Maximum number of mutation tool calls allowed per agent turn.
const DEFAULT_MUTATION_CAP: usize = 20;

/// Tool names that do not count toward the mutation cap.
const READ_ONLY_TOOLS: &[&str] = &[
    "list_rhythms",
    "get_rhythm",
    "today",
    "schedule",
    "delinquent",
    "convergence",
    "show_spoons",
    "ask_questions",
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
    file_path: PathBuf,
}

impl CadenceAgent {
    fn new(
        document: SharedDocument,
        mutation_counter: Arc<MutationCounter>,
        file_path: PathBuf,
    ) -> Self {
        Self {
            document,
            mutation_counter,
            file_path,
        }
    }
}

#[async_trait::async_trait]
impl Agent for CadenceAgent {
    async fn max_tokens(&self) -> u32 {
        64000
    }

    async fn model(&self) -> Model {
        Model::Custom("claude-haiku-4-5".to_string())
    }

    async fn system(&self) -> Option<SystemPrompt> {
        Some(SystemPrompt::String(DEFAULT_SYSTEM_PROMPT.to_string()))
    }

    fn stream_label(&self) -> String {
        "CadenceAgent".to_string()
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
                self.file_path.clone(),
            )),
            Arc::new(UpdateRhythmTool::new(
                Arc::clone(&self.document),
                self.file_path.clone(),
            )),
            Arc::new(DeleteRhythmTool::new(
                Arc::clone(&self.document),
                self.file_path.clone(),
            )),
            Arc::new(MarkDoneTool::new(
                Arc::clone(&self.document),
                self.file_path.clone(),
            )),
            Arc::new(DeferRhythmTool::new(
                Arc::clone(&self.document),
                self.file_path.clone(),
            )),
            Arc::new(SetSpoonsTool::new(
                Arc::clone(&self.document),
                self.file_path.clone(),
            )),
            Arc::new(AskQuestionsTool),
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
                    cache_control: Some(CacheControlEphemeral::new()),
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
                    let mut block = match result {
                        Ok(block) => block,
                        Err(block) => block.with_error(true),
                    };
                    if block.cache_control.is_none() {
                        block.cache_control = Some(CacheControlEphemeral::new());
                    }
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
    file_path: PathBuf,
}

impl AddRhythmTool {
    fn new(document: SharedDocument, file_path: PathBuf) -> Self {
        Self {
            document,
            file_path,
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
    file_path: PathBuf,
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
            save_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Created rhythm")
        {
            return err;
        }

        tool_ok(
            &tool_use.id,
            format!("Rhythm created: {description} (ID: {id})"),
        )
    }
}

///////////////////////////////////////// UpdateRhythmTool /////////////////////////////////////////

struct UpdateRhythmTool {
    document: SharedDocument,
    file_path: PathBuf,
}

impl UpdateRhythmTool {
    fn new(document: SharedDocument, file_path: PathBuf) -> Self {
        Self {
            document,
            file_path,
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
    file_path: PathBuf,
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
            save_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Updated rhythm")
        {
            return err;
        }

        tool_ok(&tool_use.id, format!("Rhythm {id_str} updated"))
    }
}

///////////////////////////////////////// DeleteRhythmTool /////////////////////////////////////////

struct DeleteRhythmTool {
    document: SharedDocument,
    file_path: PathBuf,
}

impl DeleteRhythmTool {
    fn new(document: SharedDocument, file_path: PathBuf) -> Self {
        Self {
            document,
            file_path,
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
    file_path: PathBuf,
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
            save_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Deleted rhythm")
        {
            return err;
        }

        tool_ok(&tool_use.id, format!("Rhythm {id_str} deleted"))
    }
}

/////////////////////////////////////////// MarkDoneTool ///////////////////////////////////////////

struct MarkDoneTool {
    document: SharedDocument,
    file_path: PathBuf,
}

impl MarkDoneTool {
    fn new(document: SharedDocument, file_path: PathBuf) -> Self {
        Self {
            document,
            file_path,
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
    file_path: PathBuf,
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
            save_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Marked done")
        {
            return err;
        }

        tool_ok(&tool_use.id, results.join("\n"))
    }
}

///////////////////////////////////////// DeferRhythmTool //////////////////////////////////////////

struct DeferRhythmTool {
    document: SharedDocument,
    file_path: PathBuf,
}

impl DeferRhythmTool {
    fn new(document: SharedDocument, file_path: PathBuf) -> Self {
        Self {
            document,
            file_path,
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
    file_path: PathBuf,
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
        if let Err(err) =
            save_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Deferred rhythm")
        {
            return err;
        }

        tool_ok(&tool_use.id, format!("Rhythm {id_str} deferred"))
    }
}

/////////////////////////////////////////// SetSpoonsTool //////////////////////////////////////////

struct SetSpoonsTool {
    document: SharedDocument,
    file_path: PathBuf,
}

impl SetSpoonsTool {
    fn new(document: SharedDocument, file_path: PathBuf) -> Self {
        Self {
            document,
            file_path,
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
    file_path: PathBuf,
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
            save_document_or_tool_err(&tool_use.id, &self.file_path, &doc, "Set spoons")
        {
            return err;
        }

        tool_ok(&tool_use.id, format!("Spoons set to {value} for {date}"))
    }
}

///////////////////////////////////////// AskQuestionsTool /////////////////////////////////////////

/// Tool that allows the agent to ask the user questions.
struct AskQuestionsTool;

impl<A: Agent> Tool<A> for AskQuestionsTool {
    fn name(&self) -> String {
        "ask_questions".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        cadence_tool_callback(AskQuestionsCallback)
    }

    fn to_param(&self) -> ToolUnionParam {
        custom_tool_param(
            "ask_questions",
            "Ask the user a series of questions one at a time.  Returns the answers.",
            json_schema(
                &[(
                    "questions",
                    "Array of questions to ask the user",
                    serde_json::json!({"type": "array", "items": {"type": "string"}}),
                )],
                &["questions"],
            ),
        )
    }
}

struct AskQuestionsCallback;

#[async_trait::async_trait]
impl<A: Agent> CadenceToolLogic<A> for AskQuestionsCallback {
    async fn compute(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let questions = match tool_use
            .input
            .get("questions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "Missing or invalid 'questions'".to_string())
        {
            Ok(q) => q.clone(),
            Err(e) => return tool_err(&tool_use.id, e),
        };

        if questions.is_empty() {
            return tool_err(&tool_use.id, "No questions provided");
        }

        let mut answers: Vec<(String, String)> = Vec::new();

        for q_val in &questions {
            let question = match q_val.as_str() {
                Some(s) => s,
                None => {
                    return tool_err(&tool_use.id, "Question must be a string");
                }
            };
            println!("\n[Question from agent]: {question}");
            match read_user_input() {
                Ok(Some(answer)) => {
                    answers.push((question.to_string(), answer));
                }
                Ok(None) => {
                    return tool_err(&tool_use.id, "End of input while answering questions");
                }
                Err(e) => {
                    return tool_err(&tool_use.id, format!("Error reading input: {e}"));
                }
            }
        }

        let response = answers
            .iter()
            .map(|(q, a)| format!("Q: {q}\nA: {a}"))
            .collect::<Vec<_>>()
            .join("\n\n");

        tool_ok(&tool_use.id, response)
    }
}

////////////////////////////////////////////// REPL ////////////////////////////////////////////////

/// Creates a user message from text input.
fn create_user_message(text: &str) -> MessageParam {
    MessageParam {
        role: MessageRole::User,
        content: MessageParamContent::Array(vec![ContentBlock::Text(TextBlock {
            text: text.to_string(),
            citations: None,
            cache_control: None,
        })]),
    }
}

/// Reads a line of input from the user.
fn read_user_input() -> std::io::Result<Option<String>> {
    print!("> ");
    std::io::stdout().flush()?;

    let stdin = std::io::stdin();
    let mut line = String::new();
    let bytes_read = stdin.lock().read_line(&mut line)?;

    if bytes_read == 0 {
        return Ok(None);
    }

    Ok(Some(line.trim().to_string()))
}

const HELP_TEXT: &str = "Commands:
  /today              Show today's items
  /list               List all rhythms
  /delinquent         Show delinquent rhythms
  /quit | /exit       End session";

#[derive(Debug, Clone)]
enum Command {
    Today,
    List,
    Delinquent,
    Help,
    Quit,
    Unknown(String),
}

fn parse_command(input: &str) -> Option<Command> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let cmd = trimmed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let command = match cmd.as_str() {
        "/today" => Command::Today,
        "/list" => Command::List,
        "/delinquent" => Command::Delinquent,
        "/help" => Command::Help,
        "/quit" | "/exit" => Command::Quit,
        other => Command::Unknown(format!("unknown command: {other}")),
    };
    Some(command)
}

fn handle_command(
    command: Command,
    document: &SharedDocument,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::Today => {
            let doc = document.lock().unwrap();
            let user_tz = document_timezone(&doc).unwrap_or(Tz::UTC);
            let items = today_items(&doc)?;
            println!(
                "{}",
                render_today_items(&items, &user_tz, TodayRenderOptions::default())
            );
        }
        Command::List => {
            let doc = document.lock().unwrap();
            let rhythms = list_rhythms(&doc, None, None);
            if rhythms.is_empty() {
                println!("No rhythms found.");
            } else {
                for rhythm in rhythms {
                    println!("{}: {} - {}", rhythm.id, rhythm.description, rhythm.rhythm);
                }
            }
        }
        Command::Delinquent => {
            let doc = document.lock().unwrap();
            let items = delinquent_items(&doc)?;
            println!("{}", render_delinquent_items(&items));
        }
        Command::Help => {
            println!("{HELP_TEXT}");
        }
        Command::Quit => return Ok(false),
        Command::Unknown(message) => {
            println!("{message}");
            println!("Type /help for commands.");
        }
    }
    Ok(true)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (options, _free) = CliOptions::from_command_line_relaxed(
        "Usage: cadence-agent [--file PATH] [--mutation-cap N]",
    );

    let file_path = options
        .file
        .map(PathBuf::from)
        .unwrap_or_else(default_file_path);

    let document: SharedDocument =
        Arc::new(Mutex::new(load_document(&file_path).map_err(|e| {
            format!("Failed to load {}: {e}", file_path.display())
        })?));

    let client = Anthropic::new(None)?;
    // TODO(rescrv):  Proper haiku 4.5 rates; this is opus 4.5.
    let budget = Arc::new(Budget::new_with_rates(1_000_000_000, 500, 2500, 625, 50));

    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_clone = interrupted.clone();
    ctrlc::set_handler(move || {
        interrupted_clone.store(true, Ordering::Relaxed);
    })?;

    let mutation_counter = Arc::new(MutationCounter::new(
        options.mutation_cap.unwrap_or(DEFAULT_MUTATION_CAP),
    ));
    let mut messages: Vec<MessageParam> = Vec::new();

    println!("=== Cadence Agent ===");
    println!("File: {}", file_path.display());
    println!("Chat with the assistant to manage your rhythms.");
    println!("Type '/help' for commands.");
    println!("Type 'quit' or 'exit' to end the session.");
    println!("Press Ctrl+C to interrupt a response.\n");

    loop {
        interrupted.store(false, Ordering::Relaxed);

        let input = match read_user_input()? {
            Some(input) => input,
            None => {
                println!("\nEnd of input. Exiting.");
                break;
            }
        };

        if input.is_empty() {
            continue;
        }

        let input_lower = input.to_lowercase();
        if input_lower == "quit" || input_lower == "exit" {
            println!("\nEnding session.");
            break;
        }

        if let Some(command) = parse_command(&input) {
            let keep_going = handle_command(command, &document)?;
            if !keep_going {
                println!("\nEnding session.");
                break;
            }
            continue;
        }

        messages.push(create_user_message(&input));

        mutation_counter.reset();
        let mut agent = CadenceAgent::new(
            Arc::clone(&document),
            Arc::clone(&mutation_counter),
            file_path.clone(),
        );

        let mut renderer = PlainTextRenderer::with_color_and_interrupt(true, interrupted.clone());
        let context = ();

        match agent
            .take_turn_streaming_root(&client, &mut messages, &budget, &mut renderer)
            .await
        {
            Ok(outcome) => {
                renderer.finish_response(&context);
                println!();

                if outcome.stop_reason != StopReason::EndTurn {
                    eprintln!("Note: Unexpected stop reason: {:?}", outcome.stop_reason);
                }
            }
            Err(e) => {
                renderer.print_error(&context, &e.to_string());
                println!();
            }
        }

        let mutations = mutation_counter.count.load(Ordering::Relaxed);
        if mutations > 0 {
            println!(
                "\n[{} mutation{} this turn.]",
                mutations,
                if mutations == 1 { "" } else { "s" },
            );
        }
    }

    Ok(())
}
