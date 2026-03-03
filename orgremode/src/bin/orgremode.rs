//! Interactive planning session agent for building org-mode files.
//!
//! This tool provides an interactive chat interface with an AI agent to help plan
//! and structure work. The conversation produces an org-mode document as output.
//!
//! Usage: `orgremode-planning-session plan.org`
//!
//! The agent has access to org-mode manipulation tools that allow it to directly
//! modify the document structure. The document auto-saves after each agent turn.

use std::any::Any;
use std::io;
use std::io::BufRead;
use std::io::Write;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use arrrg::CommandLine;
use claudius::Agent;
use claudius::AgentStreamContext;
use claudius::Anthropic;
use claudius::Budget;
use claudius::CacheControlEphemeral;
use claudius::ContentBlock;
use claudius::FileSystem;
use claudius::IntermediateToolResult;
use claudius::Message;
use claudius::MessageParam;
use claudius::MessageParamContent;
use claudius::MessageRole;
use claudius::Model;
use claudius::MountHierarchy;
use claudius::Permissions;
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
use claudius::ToolSearchFileSystem;
use claudius::ToolTextEditor20250728;
use claudius::ToolUnionParam;
use claudius::ToolUseBlock;
use orgremode::BodyUpdateMode;
use orgremode::DateTime;
use orgremode::DeleteStrategy;
use orgremode::Document;
use orgremode::Headline;
use orgremode::InsertPosition;
use orgremode::NodePath;
use orgremode::Object;
use orgremode::PlanningType;
use orgremode::Priority;
use orgremode::Text;
use orgremode::Timestamp;
use orgremode::TimestampType;
use orgremode::TodoKeyword;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use utf8path::Path;

const DEFAULT_SYSTEM_PROMPT: &str = include_str!("../../prompts/default-system.md");
const ONESHOT_SYSTEM_PROMPT: &str = r#"<one-shot-mode>
The user requested one-shot mode. You must either complete the entire request in a single turn
or stop and return a direct error explaining what is missing and how to improve the request, then
ask the user to retry. The user is making a pointed ask and will not follow up unless the work is
not satisfactory. The most efficient path is to complete the work thoroughly on the first pass.
Do not ask the user questions, request interactive clarification, or ask whether the user wants
follow-up work.
</one-shot-mode>"#;
const SKILL_HEADER: &str = r#"---
name: orgremode-oneshot
description: Use orgremode --oneshot effectively by writing a single, pointed, complete request that minimizes follow-up.
---
"#;

const HELP_TEXT: &str = "Commands:
  /show | /doc          Show current document (respects /focus)
  /outline              Show headline outline
  /focus <id|path|text> Focus view to a subtree
  /back                 Clear focus
  /diff                 Run `git diff`
  /status               Run `git status`
  /git <args>           Run `git <args>`
  /undo                 Undo last change
  /redo                 Redo last undone change
  /quit | /exit         End session";

#[derive(arrrg_derive::CommandLine, Debug, Default, Eq, PartialEq)]
struct CliOptions {
    #[arrrg(
        flag,
        "Print a SKILL.md template for using orgremode --oneshot effectively and exit"
    )]
    skill: bool,

    #[arrrg(
        optional,
        "One-shot mode seed prompt (required to enable). Do it all in one turn or return an error",
        "TEXT"
    )]
    oneshot: Option<String>,

    #[arrrg(
        flag,
        "Preview agent output without writing the file (oneshot mode only)"
    )]
    dry_run: bool,

    #[arrrg(optional, "Override the default mutation cap per turn", "N")]
    mutation_cap: Option<usize>,

    #[arrrg(
        optional,
        "Read-only query mode: answer a question about the document without mutating it",
        "TEXT"
    )]
    query: Option<String>,
}

/// Configuration for the planning session agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningSessionConfig {
    /// The system prompt that guides Claude's planning behavior.
    pub system_prompt: String,
    /// Enforce one-shot behavior (no interactive clarification).
    #[serde(default)]
    pub oneshot: bool,
    /// Preview agent output without writing the file (oneshot mode only).
    #[serde(default)]
    pub dry_run: bool,
    /// Override the default mutation cap per turn.
    #[serde(default)]
    pub mutation_cap: Option<usize>,
    /// The model to use for the session.
    #[serde(default = "default_model")]
    pub model: String,
    /// Maximum tokens for responses.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Label for streaming output.
    #[serde(default = "default_stream_label")]
    pub stream_label: String,
}

fn default_model() -> String {
    "claude-opus-4-5".to_string()
}

fn default_max_tokens() -> u32 {
    64000
}

fn default_stream_label() -> String {
    "PlanningSession".to_string()
}

impl Default for PlanningSessionConfig {
    fn default() -> Self {
        Self {
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            oneshot: false,
            dry_run: false,
            mutation_cap: None,
            model: default_model(),
            max_tokens: default_max_tokens(),
            stream_label: default_stream_label(),
        }
    }
}

/// Shared document state for the planning session.
type SharedDocument = Arc<Mutex<Document>>;

/// Renderer for one-shot mode that outputs only streamed assistant text.
struct OneShotTextOnlyRenderer {
    interrupted: Arc<AtomicBool>,
    saw_text_since_tool: bool,
    last_char_was_newline: bool,
    tool_errors: Vec<OneShotToolError>,
    current_tool_name: Option<String>,
    current_tool_is_error: bool,
    current_tool_error_text: String,
}

/// A tool error captured during one-shot execution.
struct OneShotToolError {
    tool_name: String,
    message: String,
}

impl OneShotTextOnlyRenderer {
    fn new(interrupted: Arc<AtomicBool>) -> Self {
        Self {
            interrupted,
            saw_text_since_tool: false,
            last_char_was_newline: true,
            tool_errors: Vec::new(),
            current_tool_name: None,
            current_tool_is_error: false,
            current_tool_error_text: String::new(),
        }
    }

    /// Format collected tool errors for structured reporting.
    fn format_tool_errors(&self) -> Option<String> {
        if self.tool_errors.is_empty() {
            return None;
        }
        let mut report = String::from("oneshot: tool errors encountered during execution:\n");
        for (i, err) in self.tool_errors.iter().enumerate() {
            report.push_str(&format!(
                "  {}. [{}] {}\n",
                i + 1,
                err.tool_name,
                err.message
            ));
        }
        Some(report)
    }
}

impl Renderer for OneShotTextOnlyRenderer {
    fn print_text(&mut self, _context: &dyn claudius::StreamContext, text: &str) {
        print!("{text}");
        let _ = io::stdout().flush();
        if !text.is_empty() {
            self.saw_text_since_tool = true;
            self.last_char_was_newline = text.ends_with('\n');
        }
    }

    fn print_thinking(&mut self, _context: &dyn claudius::StreamContext, _text: &str) {}

    fn print_error(&mut self, _context: &dyn claudius::StreamContext, _error: &str) {}

    fn print_info(&mut self, _context: &dyn claudius::StreamContext, _info: &str) {}

    fn start_tool_use(&mut self, _context: &dyn claudius::StreamContext, name: &str, _id: &str) {
        if self.saw_text_since_tool && !self.last_char_was_newline {
            println!();
            let _ = io::stdout().flush();
            self.last_char_was_newline = true;
        }
        self.saw_text_since_tool = false;
        self.current_tool_name = Some(name.to_string());
    }

    fn print_tool_input(&mut self, _context: &dyn claudius::StreamContext, _partial_json: &str) {}

    fn finish_tool_use(&mut self, _context: &dyn claudius::StreamContext) {}

    fn start_tool_result(
        &mut self,
        _context: &dyn claudius::StreamContext,
        _tool_use_id: &str,
        is_error: bool,
    ) {
        self.current_tool_is_error = is_error;
        self.current_tool_error_text.clear();
    }

    fn print_tool_result_text(&mut self, _context: &dyn claudius::StreamContext, text: &str) {
        if self.current_tool_is_error {
            self.current_tool_error_text.push_str(text);
        }
    }

    fn finish_tool_result(&mut self, _context: &dyn claudius::StreamContext) {
        if self.current_tool_is_error && !self.current_tool_error_text.is_empty() {
            let tool_name = self
                .current_tool_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            self.tool_errors.push(OneShotToolError {
                tool_name,
                message: self.current_tool_error_text.clone(),
            });
        }
        self.current_tool_is_error = false;
        self.current_tool_error_text.clear();
    }

    fn finish_response(&mut self, _context: &dyn claudius::StreamContext) {
        if !self.last_char_was_newline {
            println!();
            let _ = io::stdout().flush();
            self.last_char_was_newline = true;
        }
    }

    fn should_interrupt(&self) -> bool {
        self.interrupted.load(Ordering::Relaxed)
    }
}

/// Maximum number of mutation tool calls allowed per agent turn.
const DEFAULT_MUTATION_CAP: usize = 20;

/// Tool names that do not count toward the mutation cap.
const READ_ONLY_TOOLS: &[&str] = &[
    "get_document",
    "document_schema",
    "list_headlines",
    "get_headline",
    "find_headlines",
    "validate_document",
    "ensure_id",
    "ask_questions",
];

/// Tracks mutation tool calls within a single agent turn.
///
/// Shared across all mutation tool callbacks.  Reset to zero before each turn.
/// When the count reaches the cap, mutation tools return errors instructing the
/// agent to stop and check in with the user.
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

#[derive(Debug, Clone)]
struct SessionState {
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
    focus: Option<NodePath>,
}

impl SessionState {
    fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            focus: None,
        }
    }
}

/// Agent for interactive planning sessions, configured by PlanningSessionConfig.
struct PlanningSessionAgent<'a> {
    config: &'a PlanningSessionConfig,
    document: SharedDocument,
    mutation_counter: Arc<MutationCounter>,
    filesystem: Option<MountHierarchy>,
}

impl<'a> PlanningSessionAgent<'a> {
    fn new(
        config: &'a PlanningSessionConfig,
        document: SharedDocument,
        mutation_counter: Arc<MutationCounter>,
    ) -> Self {
        Self {
            config,
            document,
            mutation_counter,
            filesystem: None,
        }
    }

    fn with_filesystem(
        mut self,
        root: Option<Path<'static>>,
        skills: Option<Path<'static>>,
    ) -> Self {
        if root.is_none() && skills.is_none() {
            return self;
        }
        let mut hierarchy = MountHierarchy::default();
        if let Some(root) = root {
            hierarchy
                .mount("/".into(), Permissions::ReadOnly, root)
                .expect("mount at / should never fail");
        }
        if let Some(skills) = skills {
            hierarchy
                .mount("/.skills".into(), Permissions::ReadOnly, skills)
                .expect("mount at /.skills should never fail");
        }
        self.filesystem = Some(hierarchy);
        self
    }
}

#[async_trait::async_trait]
impl Agent for PlanningSessionAgent<'_> {
    async fn max_tokens(&self) -> u32 {
        self.config.max_tokens
    }

    async fn model(&self) -> Model {
        Model::Custom(self.config.model.clone())
    }

    async fn system(&self) -> Option<SystemPrompt> {
        Some(SystemPrompt::String(self.config.system_prompt.clone()))
    }

    fn stream_label(&self) -> String {
        self.config.stream_label.clone()
    }

    async fn tools(&self) -> Vec<Arc<dyn Tool<Self>>> {
        let mut tools: Vec<Arc<dyn Tool<Self>>> = vec![
            Arc::new(InsertHeadlineTool::new(Arc::clone(&self.document))),
            Arc::new(SetStateTool::new(Arc::clone(&self.document))),
            Arc::new(SetPriorityTool::new(Arc::clone(&self.document))),
            Arc::new(SetTitleTool::new(Arc::clone(&self.document))),
            Arc::new(SetTagsTool::new(Arc::clone(&self.document))),
            Arc::new(SetPlanningTool::new(Arc::clone(&self.document))),
            Arc::new(SetPropertyTool::new(Arc::clone(&self.document))),
            Arc::new(UpdateBodyTool::new(Arc::clone(&self.document))),
            Arc::new(DeleteNodeTool::new(Arc::clone(&self.document))),
            Arc::new(RefileNodeTool::new(Arc::clone(&self.document))),
            Arc::new(GetDocumentTool::new(Arc::clone(&self.document))),
            Arc::new(DocumentSchemaTool::new(Arc::clone(&self.document))),
            Arc::new(ListHeadlinesTool::new(Arc::clone(&self.document))),
            Arc::new(GetHeadlineTool::new(Arc::clone(&self.document))),
            Arc::new(FindHeadlinesTool::new(Arc::clone(&self.document))),
            Arc::new(DeleteRangeTool::new(Arc::clone(&self.document))),
            Arc::new(ClearDocumentTool::new(Arc::clone(&self.document))),
            Arc::new(ValidateDocumentTool::new(Arc::clone(&self.document))),
            Arc::new(EnsureIdTool::new(Arc::clone(&self.document))),
        ];
        if !self.config.oneshot {
            tools.push(Arc::new(AskQuestionsTool));
        }
        if self.filesystem.is_some() {
            tools.push(Arc::new(ToolTextEditor20250728::new()));
            tools.push(Arc::new(ToolSearchFileSystem));
        }
        tools
    }

    async fn filesystem(&self) -> Option<&dyn FileSystem> {
        self.filesystem.as_ref().map(|fs| fs as &dyn FileSystem)
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

////////////////////////////////////////////// Tools //////////////////////////////////////////////

/// Intermediate result for org-mode tools.
struct OrgToolResult {
    tool_use_id: String,
    result: Result<String, String>,
}

impl IntermediateToolResult for OrgToolResult {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn apply_org_tool_result(intermediate: Box<dyn IntermediateToolResult>) -> ToolResult {
    let result = intermediate
        .as_any()
        .downcast_ref::<OrgToolResult>()
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

fn org_tool_ok(tool_use_id: &str, message: impl Into<String>) -> Box<dyn IntermediateToolResult> {
    Box::new(OrgToolResult {
        tool_use_id: tool_use_id.to_string(),
        result: Ok(message.into()),
    })
}

fn org_tool_err(tool_use_id: &str, message: impl Into<String>) -> Box<dyn IntermediateToolResult> {
    Box::new(OrgToolResult {
        tool_use_id: tool_use_id.to_string(),
        result: Err(message.into()),
    })
}

fn get_required_str<'a>(tool_use: &'a ToolUseBlock, field: &str) -> Result<&'a str, String> {
    tool_use
        .input
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing or invalid '{}'", field))
}

fn get_optional_str<'a>(tool_use: &'a ToolUseBlock, field: &str) -> Option<&'a str> {
    tool_use.input.get(field).and_then(|v| v.as_str())
}

fn get_optional_bool(tool_use: &ToolUseBlock, field: &str) -> Option<bool> {
    tool_use.input.get(field).and_then(|v| v.as_bool())
}

fn get_required_i64(tool_use: &ToolUseBlock, field: &str) -> Result<i64, String> {
    tool_use
        .input
        .get(field)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| format!("Missing or invalid '{}'", field))
}

fn get_required_u64(tool_use: &ToolUseBlock, field: &str) -> Result<u64, String> {
    tool_use
        .input
        .get(field)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("Missing or invalid '{}'", field))
}

fn get_optional_u64(tool_use: &ToolUseBlock, field: &str) -> Option<u64> {
    tool_use.input.get(field).and_then(|v| v.as_u64())
}

fn get_required_usize_array(tool_use: &ToolUseBlock, field: &str) -> Result<Vec<usize>, String> {
    let array = tool_use
        .input
        .get(field)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Missing or invalid '{}'", field))?;
    let mut out = Vec::with_capacity(array.len());
    for v in array {
        let n = v
            .as_u64()
            .ok_or_else(|| format!("Invalid value in '{}'", field))?;
        out.push(n as usize);
    }
    if out.is_empty() {
        return Err(format!("'{}' cannot be empty", field));
    }
    Ok(out)
}

fn get_optional_usize_array(
    tool_use: &ToolUseBlock,
    field: &str,
) -> Result<Option<Vec<usize>>, String> {
    let Some(value) = tool_use.input.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let array = value
        .as_array()
        .ok_or_else(|| format!("Missing or invalid '{}'", field))?;
    if array.is_empty() {
        return Err(format!("'{}' cannot be empty", field));
    }
    let mut out = Vec::with_capacity(array.len());
    for v in array {
        let n = v
            .as_u64()
            .ok_or_else(|| format!("Invalid value in '{}'", field))?;
        out.push(n as usize);
    }
    Ok(Some(out))
}

fn get_required_string_array(tool_use: &ToolUseBlock, field: &str) -> Result<Vec<String>, String> {
    let array = tool_use
        .input
        .get(field)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Missing or invalid '{}'", field))?;
    let mut out = Vec::with_capacity(array.len());
    for v in array {
        let s = v
            .as_str()
            .ok_or_else(|| format!("Invalid value in '{}'", field))?;
        out.push(s.to_string());
    }
    Ok(out)
}

fn get_optional_string_array(tool_use: &ToolUseBlock, field: &str) -> Option<Vec<String>> {
    let array = tool_use.input.get(field)?.as_array()?;
    let mut out = Vec::with_capacity(array.len());
    for v in array {
        out.push(v.as_str()?.to_string());
    }
    Some(out)
}

fn parse_insert_position(value: Option<&Value>) -> Result<InsertPosition, String> {
    match value {
        Some(Value::String(s)) if s == "prepend" => Ok(InsertPosition::Prepend),
        Some(Value::String(s)) if s == "append" => Ok(InsertPosition::Append),
        Some(Value::Number(n)) => n
            .as_u64()
            .map(|n| InsertPosition::Index(n as usize))
            .ok_or_else(|| "Invalid position value".to_string()),
        None => Ok(InsertPosition::Append),
        _ => Err("Invalid position value".to_string()),
    }
}

fn parse_body_update_mode(value: Option<&Value>) -> Result<BodyUpdateMode, String> {
    match value.and_then(|v| v.as_str()) {
        Some("append") => Ok(BodyUpdateMode::Append),
        Some("prepend") => Ok(BodyUpdateMode::Prepend),
        Some("replace") | None => Ok(BodyUpdateMode::Replace),
        _ => Err("Invalid mode value".to_string()),
    }
}

fn parse_delete_strategy(value: Option<&Value>) -> Result<DeleteStrategy, String> {
    match value.and_then(|v| v.as_str()) {
        Some("promote") => Ok(DeleteStrategy::PromoteChildren),
        Some("cascade") | None => Ok(DeleteStrategy::Cascade),
        _ => Err("Invalid delete strategy".to_string()),
    }
}

fn parse_planning_type(value: Option<&Value>) -> Result<PlanningType, String> {
    match value.and_then(|v| v.as_str()) {
        Some("deadline") => Ok(PlanningType::Deadline),
        Some("scheduled") => Ok(PlanningType::Scheduled),
        Some("closed") => Ok(PlanningType::Closed),
        _ => Err("Invalid planning_type".to_string()),
    }
}

fn validate_date_fields(year: i32, month: u8, day: u8) -> Result<(), String> {
    if !(1..=12).contains(&month) {
        return Err("month must be 1-12".to_string());
    }
    if !(1..=31).contains(&day) {
        return Err("day must be 1-31".to_string());
    }
    if year < 1 {
        return Err("year must be >= 1".to_string());
    }
    Ok(())
}

fn validate_time_fields(hour: Option<u8>, minute: Option<u8>) -> Result<(), String> {
    if let Some(h) = hour
        && h > 23
    {
        return Err("hour must be 0-23".to_string());
    }
    if let Some(m) = minute
        && m > 59
    {
        return Err("minute must be 0-59".to_string());
    }
    Ok(())
}

fn parse_datetime_value(value: &Value) -> Result<DateTime, String> {
    let year = value
        .get("year")
        .and_then(|v| v.as_i64())
        .ok_or("missing year")? as i32;
    let month = value
        .get("month")
        .and_then(|v| v.as_u64())
        .ok_or("missing month")? as u8;
    let day = value
        .get("day")
        .and_then(|v| v.as_u64())
        .ok_or("missing day")? as u8;
    let hour = value.get("hour").and_then(|v| v.as_u64()).map(|h| h as u8);
    let minute = value
        .get("minute")
        .and_then(|v| v.as_u64())
        .map(|m| m as u8);

    validate_date_fields(year, month, day)?;
    validate_time_fields(hour, minute)?;

    Ok(DateTime {
        year,
        month,
        day,
        dayname: None,
        hour,
        minute,
        end_hour: None,
        end_minute: None,
    })
}

//////////////////////////////////////////// InsertHeadlineTool ///////////////////////////////////

struct InsertHeadlineTool {
    document: SharedDocument,
}

impl InsertHeadlineTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for InsertHeadlineTool {
    fn name(&self) -> String {
        "insert_headline".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(InsertHeadlineCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        let datetime_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "year": {"type": "integer"},
                "month": {"type": "integer", "minimum": 1, "maximum": 12},
                "day": {"type": "integer", "minimum": 1, "maximum": 31},
                "hour": {"type": "integer", "minimum": 0, "maximum": 23},
                "minute": {"type": "integer", "minimum": 0, "maximum": 59}
            },
            "required": ["year", "month", "day"]
        });
        ToolUnionParam::CustomTool(ToolParam {
            name: "insert_headline".to_string(),
            description: Some(
                "Insert a new headline into the org-mode document. Returns the assigned ID."
                    .to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "parent_id",
                        "ID of the parent headline (null for top-level)",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                    (
                        "position",
                        "Where to insert: 'prepend', 'append', or index number",
                        serde_json::json!({"type": ["string", "integer"]}),
                    ),
                    (
                        "title",
                        "Title text for the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "preferred_id",
                        "Preferred ID to assign (auto-generated if not provided or already taken)",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                    (
                        "state",
                        "TODO state: 'TODO', 'DONE', or null",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                    (
                        "priority",
                        "Priority: 'A', 'B', 'C' (or any uppercase letter), or null",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                    (
                        "tags",
                        "Array of tag strings",
                        serde_json::json!({"type": "array", "items": {"type": "string"}}),
                    ),
                    (
                        "body",
                        "Body content for the headline section",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                    ("deadline", "Deadline timestamp", datetime_schema.clone()),
                    ("scheduled", "Scheduled timestamp", datetime_schema),
                ],
                &["title"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct InsertHeadlineCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for InsertHeadlineCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let parent_id = get_optional_str(tool_use, "parent_id");
        let position = match parse_insert_position(tool_use.input.get("position")) {
            Ok(position) => position,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let title = match get_required_str(tool_use, "title") {
            Ok(title) => title,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let preferred_id = get_optional_str(tool_use, "preferred_id");
        let state = get_optional_str(tool_use, "state");
        let priority_str = get_optional_str(tool_use, "priority");
        let tags = get_optional_string_array(tool_use, "tags");
        let body = get_optional_str(tool_use, "body");
        let deadline = tool_use.input.get("deadline");
        let scheduled = tool_use.input.get("scheduled");

        let title_objects = vec![Object::Text(Text {
            value: title.to_string(),
        })];

        let mut doc = self.document.lock().unwrap();

        // Insert the headline
        let path = match orgremode::insert_headline(&mut doc, parent_id, position, title_objects) {
            Ok(path) => path,
            Err(e) => return org_tool_err(&tool_use.id, e.to_string()),
        };

        // Ensure ID (this also handles preferred_id)
        let id = match orgremode::ensure_id(&mut doc, &path, preferred_id) {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e.to_string()),
        };

        // Set state if provided
        if let Some(state_str) = state {
            let is_done = state_str.eq_ignore_ascii_case("DONE");
            let keyword = match TodoKeyword::new(state_str, is_done) {
                Ok(kw) => Some(kw),
                Err(e) => return org_tool_err(&tool_use.id, e.to_string()),
            };
            if let Err(e) = orgremode::set_state_by_id(&mut doc, &id, keyword) {
                return org_tool_err(&tool_use.id, e.to_string());
            }
        }

        // Set priority if provided
        if let Some(priority_char) = priority_str {
            let ch = priority_char.chars().next().unwrap_or('A');
            let priority = match orgremode::Priority::new(ch) {
                Ok(p) => p,
                Err(e) => return org_tool_err(&tool_use.id, e.to_string()),
            };
            if let Err(e) = orgremode::set_priority_by_id(&mut doc, &id, Some(priority)) {
                return org_tool_err(&tool_use.id, e.to_string());
            }
        }

        // Set tags if provided
        if let Some(tag_list) = tags {
            let tag_refs: Vec<&str> = tag_list.iter().map(|s| s.as_str()).collect();
            if let Err(e) = orgremode::set_tags_by_id(&mut doc, &id, &tag_refs) {
                return org_tool_err(&tool_use.id, e.to_string());
            }
        }

        // Set body if provided
        if let Some(body_text) = body
            && let Err(e) =
                orgremode::update_body_by_id(&mut doc, &id, body_text, BodyUpdateMode::Replace)
        {
            return org_tool_err(&tool_use.id, e.to_string());
        }

        // Set deadline if provided
        if let Some(deadline_val) = deadline
            && !deadline_val.is_null()
        {
            match parse_datetime_value(deadline_val) {
                Ok(dt) => {
                    let ts = Timestamp {
                        timestamp_type: TimestampType::Active,
                        start: dt,
                        end: None,
                        repeater: None,
                        warning: None,
                    };
                    if let Err(e) = orgremode::set_planning_by_id(
                        &mut doc,
                        &id,
                        PlanningType::Deadline,
                        Some(ts),
                    ) {
                        return org_tool_err(&tool_use.id, e.to_string());
                    }
                }
                Err(e) => return org_tool_err(&tool_use.id, format!("deadline: {e}")),
            }
        }

        // Set scheduled if provided
        if let Some(scheduled_val) = scheduled
            && !scheduled_val.is_null()
        {
            match parse_datetime_value(scheduled_val) {
                Ok(dt) => {
                    let ts = Timestamp {
                        timestamp_type: TimestampType::Active,
                        start: dt,
                        end: None,
                        repeater: None,
                        warning: None,
                    };
                    if let Err(e) = orgremode::set_planning_by_id(
                        &mut doc,
                        &id,
                        PlanningType::Scheduled,
                        Some(ts),
                    ) {
                        return org_tool_err(&tool_use.id, e.to_string());
                    }
                }
                Err(e) => return org_tool_err(&tool_use.id, format!("scheduled: {e}")),
            }
        }

        org_tool_ok(&tool_use.id, format!("Headline inserted with ID: {id}"))
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// SetStateTool /////////////////////////////////////////

struct SetStateTool {
    document: SharedDocument,
}

impl SetStateTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for SetStateTool {
    fn name(&self) -> String {
        "set_state".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(SetStateCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "set_state".to_string(),
            description: Some(
                "Set the TODO state of a headline (TODO, DONE, or null to clear)".to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "keyword",
                        "TODO keyword (TODO, DONE) or null to clear",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                    (
                        "is_done",
                        "Whether this is a done state (default: false for TODO, true for DONE)",
                        serde_json::json!({"type": "boolean"}),
                    ),
                ],
                &["id"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct SetStateCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for SetStateCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let keyword = get_optional_str(tool_use, "keyword");
        let is_done =
            get_optional_bool(tool_use, "is_done").unwrap_or_else(|| keyword == Some("DONE"));

        let keyword = if let Some(kw) = keyword {
            match TodoKeyword::new(kw, is_done) {
                Ok(keyword) => Some(keyword),
                Err(e) => return org_tool_err(&tool_use.id, e.to_string()),
            }
        } else {
            None
        };

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::set_state_by_id(&mut doc, id, keyword)
            .map(|()| "State updated".to_string())
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// SetPriorityTool ///////////////////////////////////////

struct SetPriorityTool {
    document: SharedDocument,
}

impl SetPriorityTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for SetPriorityTool {
    fn name(&self) -> String {
        "set_priority".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(SetPriorityCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "set_priority".to_string(),
            description: Some(
                "Set the priority of a headline (A, B, C or any uppercase letter, or null to clear)"
                    .to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "priority",
                        "Priority letter (A, B, C, etc.) or null to clear",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                ],
                &["id"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct SetPriorityCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for SetPriorityCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let priority_str = get_optional_str(tool_use, "priority");

        let priority = if let Some(p) = priority_str {
            let ch = p.chars().next().unwrap_or('A');
            match Priority::new(ch) {
                Ok(priority) => Some(priority),
                Err(e) => return org_tool_err(&tool_use.id, e.to_string()),
            }
        } else {
            None
        };

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::set_priority_by_id(&mut doc, id, priority)
            .map(|()| "Priority updated".to_string())
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// SetTitleTool /////////////////////////////////////////

struct SetTitleTool {
    document: SharedDocument,
}

impl SetTitleTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for SetTitleTool {
    fn name(&self) -> String {
        "set_title".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(SetTitleCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "set_title".to_string(),
            description: Some("Rename a headline by replacing its title text".to_string()),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "title",
                        "New title text for the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                ],
                &["id", "title"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct SetTitleCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for SetTitleCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let title = match get_required_str(tool_use, "title") {
            Ok(title) => title,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };

        let title_objects = vec![Object::Text(Text {
            value: title.to_string(),
        })];

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::set_title_by_id(&mut doc, id, title_objects)
            .map(|()| "Title updated".to_string())
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// SetTagsTool //////////////////////////////////////////

struct SetTagsTool {
    document: SharedDocument,
}

impl SetTagsTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for SetTagsTool {
    fn name(&self) -> String {
        "set_tags".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(SetTagsCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "set_tags".to_string(),
            description: Some("Set tags on a headline (replaces existing tags)".to_string()),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "tags",
                        "Array of tag strings",
                        serde_json::json!({"type": "array", "items": {"type": "string"}}),
                    ),
                ],
                &["id", "tags"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct SetTagsCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for SetTagsCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let tags = match get_required_string_array(tool_use, "tags") {
            Ok(tags) => tags,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let tags_ref: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::set_tags_by_id(&mut doc, id, &tags_ref)
            .map(|()| "Tags updated".to_string())
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// SetPlanningTool //////////////////////////////////////

struct SetPlanningTool {
    document: SharedDocument,
}

impl SetPlanningTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for SetPlanningTool {
    fn name(&self) -> String {
        "set_planning".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(SetPlanningCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "set_planning".to_string(),
            description: Some(
                "Set a planning timestamp (DEADLINE, SCHEDULED, or CLOSED) on a headline"
                    .to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "planning_type",
                        "Type: 'deadline', 'scheduled', or 'closed'",
                        serde_json::json!({"type": "string", "enum": ["deadline", "scheduled", "closed"]}),
                    ),
                    (
                        "year",
                        "Year (e.g., 2025)",
                        serde_json::json!({"type": "integer"}),
                    ),
                    (
                        "month",
                        "Month (1-12)",
                        serde_json::json!({"type": "integer"}),
                    ),
                    ("day", "Day (1-31)", serde_json::json!({"type": "integer"})),
                    (
                        "hour",
                        "Hour (0-23, optional)",
                        serde_json::json!({"type": ["integer", "null"]}),
                    ),
                    (
                        "minute",
                        "Minute (0-59, optional)",
                        serde_json::json!({"type": ["integer", "null"]}),
                    ),
                ],
                &["id", "planning_type", "year", "month", "day"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct SetPlanningCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for SetPlanningCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let planning_type = match parse_planning_type(tool_use.input.get("planning_type")) {
            Ok(planning_type) => planning_type,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };

        let year = match get_required_i64(tool_use, "year") {
            Ok(year) => year as i32,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let month = match get_required_u64(tool_use, "month") {
            Ok(month) => month as u8,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let day = match get_required_u64(tool_use, "day") {
            Ok(day) => day as u8,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let hour = get_optional_u64(tool_use, "hour").map(|h| h as u8);
        let minute = get_optional_u64(tool_use, "minute").map(|m| m as u8);

        if let Err(e) = validate_date_fields(year, month, day) {
            return org_tool_err(&tool_use.id, e);
        }
        if let Err(e) = validate_time_fields(hour, minute) {
            return org_tool_err(&tool_use.id, e);
        }

        let timestamp = Timestamp {
            timestamp_type: TimestampType::Active,
            start: DateTime {
                year,
                month,
                day,
                dayname: None,
                hour,
                minute,
                end_hour: None,
                end_minute: None,
            },
            end: None,
            repeater: None,
            warning: None,
        };

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::set_planning_by_id(&mut doc, id, planning_type, Some(timestamp))
            .map(|()| "Planning timestamp set".to_string())
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// SetPropertyTool //////////////////////////////////////

struct SetPropertyTool {
    document: SharedDocument,
}

impl SetPropertyTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for SetPropertyTool {
    fn name(&self) -> String {
        "set_property".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(SetPropertyCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "set_property".to_string(),
            description: Some(
                "Set a property on a headline (use this to set :ID: for future references)"
                    .to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "name",
                        "Property name (e.g., 'ID', 'EFFORT')",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "value",
                        "Property value",
                        serde_json::json!({"type": "string"}),
                    ),
                ],
                &["id", "name", "value"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct SetPropertyCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for SetPropertyCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let name = match get_required_str(tool_use, "name") {
            Ok(name) => name,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let value = match get_required_str(tool_use, "value") {
            Ok(value) => value,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::set_property_by_id(&mut doc, id, name, value)
            .map(|()| format!("Property {} set", name))
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// UpdateBodyTool ///////////////////////////////////////

struct UpdateBodyTool {
    document: SharedDocument,
}

impl UpdateBodyTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for UpdateBodyTool {
    fn name(&self) -> String {
        "update_body".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(UpdateBodyCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "update_body".to_string(),
            description: Some(
                "Update the body text of a headline (description, notes, etc.)".to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "content",
                        "Body content text",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "mode",
                        "Update mode: 'replace', 'append', or 'prepend'",
                        serde_json::json!({"type": "string", "enum": ["replace", "append", "prepend"]}),
                    ),
                ],
                &["id", "content"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct UpdateBodyCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for UpdateBodyCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let content = match get_required_str(tool_use, "content") {
            Ok(content) => content,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let mode = match parse_body_update_mode(tool_use.input.get("mode")) {
            Ok(mode) => mode,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::update_body_by_id(&mut doc, id, content, mode)
            .map(|()| "Body updated".to_string())
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// DeleteNodeTool ///////////////////////////////////////

struct DeleteNodeTool {
    document: SharedDocument,
}

impl DeleteNodeTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for DeleteNodeTool {
    fn name(&self) -> String {
        "delete_node".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(DeleteNodeCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "delete_node".to_string(),
            description: Some("Delete a headline from the document".to_string()),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline to delete",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "strategy",
                        "Delete strategy: 'cascade' (delete children) or 'promote' (promote children to parent)",
                        serde_json::json!({"type": "string", "enum": ["cascade", "promote"]}),
                    ),
                ],
                &["id"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct DeleteNodeCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for DeleteNodeCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let strategy = match parse_delete_strategy(tool_use.input.get("strategy")) {
            Ok(strategy) => strategy,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::delete_node_by_id(&mut doc, id, strategy)
            .map(|()| "Node deleted".to_string())
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// RefileNodeTool ///////////////////////////////////////

struct RefileNodeTool {
    document: SharedDocument,
}

impl RefileNodeTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for RefileNodeTool {
    fn name(&self) -> String {
        "refile_node".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(RefileNodeCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "refile_node".to_string(),
            description: Some(
                "Move a headline to a different location in the document".to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "ID of the headline to move",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "new_parent_id",
                        "ID of the new parent (null for top-level)",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                    (
                        "position",
                        "Where to insert: 'prepend', 'append', or index number",
                        serde_json::json!({"type": ["string", "integer"]}),
                    ),
                ],
                &["id"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct RefileNodeCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for RefileNodeCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = match get_required_str(tool_use, "id") {
            Ok(id) => id,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let new_parent_id = get_optional_str(tool_use, "new_parent_id");
        let position = match parse_insert_position(tool_use.input.get("position")) {
            Ok(position) => position,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::refile_node_by_id(&mut doc, id, new_parent_id, position)
            .map(|()| "Node refiled".to_string())
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// GetDocumentTool //////////////////////////////////////

struct GetDocumentTool {
    document: SharedDocument,
}

impl GetDocumentTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for GetDocumentTool {
    fn name(&self) -> String {
        "get_document".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(GetDocumentCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "get_document".to_string(),
            description: Some("Get the current org-mode document content as text".to_string()),
            input_schema: json_schema(&[], &[]),
            cache_control: None,
            strict: None,
        })
    }
}

struct GetDocumentCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for GetDocumentCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        let content = format!("{}", doc);

        org_tool_ok(
            &tool_use.id,
            if content.trim().is_empty() {
                "(empty document)".to_string()
            } else {
                content
            },
        )
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// DocumentSchemaTool ///////////////////////////////////

struct DocumentSchemaTool {
    document: SharedDocument,
}

impl DocumentSchemaTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for DocumentSchemaTool {
    fn name(&self) -> String {
        "document_schema".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(DocumentSchemaCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "document_schema".to_string(),
            description: Some(
                "Show the document's outline as a tree with TODO state, tags, and IDs.".to_string(),
            ),
            input_schema: json_schema(&[], &[]),
            cache_control: None,
            strict: None,
        })
    }
}

struct DocumentSchemaCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for DocumentSchemaCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        org_tool_ok(&tool_use.id, render_outline(&doc))
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// ListHeadlinesTool ////////////////////////////////////

struct ListHeadlinesTool {
    document: SharedDocument,
}

impl ListHeadlinesTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for ListHeadlinesTool {
    fn name(&self) -> String {
        "list_headlines".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(ListHeadlinesCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "list_headlines".to_string(),
            description: Some(
                "List all headlines with path, state, title, tags, and ID for safe edits."
                    .to_string(),
            ),
            input_schema: json_schema(&[], &[]),
            cache_control: None,
            strict: None,
        })
    }
}

struct ListHeadlinesCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for ListHeadlinesCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        let mut rows = Vec::new();
        collect_headline_rows(&doc.headlines, &mut Vec::new(), &mut rows);
        if rows.is_empty() {
            return org_tool_ok(&tool_use.id, "No headlines.");
        }
        let mut out = String::from("path | state | level | id | title\n");
        out.push_str("-----|-------|-------|----|------\n");
        for (path, state, title, id, level) in rows {
            let id_display = id.unwrap_or_else(|| "-".to_string());
            out.push_str(&format!(
                "{path} | {state} | {level} | {id_display} | {title}\n"
            ));
        }
        org_tool_ok(&tool_use.id, out)
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// GetHeadlineTool //////////////////////////////////////

struct GetHeadlineTool {
    document: SharedDocument,
}

impl GetHeadlineTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for GetHeadlineTool {
    fn name(&self) -> String {
        "get_headline".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(GetHeadlineCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "get_headline".to_string(),
            description: Some("Inspect one headline by ID or path before mutating it.".to_string()),
            input_schema: json_schema(
                &[
                    (
                        "id",
                        "Headline ID selector",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                    (
                        "path",
                        "Path selector as array of indices",
                        serde_json::json!({"type": ["array", "null"], "items": {"type": "integer"}}),
                    ),
                    (
                        "include_subtree",
                        "If true, include full subtree content; if false, metadata only",
                        serde_json::json!({"type": "boolean"}),
                    ),
                ],
                &[],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct GetHeadlineCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for GetHeadlineCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let id = get_optional_str(tool_use, "id");
        let path = match get_optional_usize_array(tool_use, "path") {
            Ok(path) => path,
            Err(err) => return org_tool_err(&tool_use.id, err),
        };
        let include_subtree = get_optional_bool(tool_use, "include_subtree").unwrap_or(true);

        let doc = self.document.lock().unwrap();
        let path = match resolve_headline_path_by_id_or_path(&doc, id, path.as_deref()) {
            Ok(path) => path,
            Err(err) => return org_tool_err(&tool_use.id, err),
        };
        let Some(headline) = headline_by_path(&doc.headlines, &path.0) else {
            return org_tool_err(&tool_use.id, "resolved path no longer exists");
        };

        let mut out = String::new();
        out.push_str(&format!("path: {}\n", format_node_path(&path.0)));
        out.push_str(&format!(
            "state: {}\n",
            headline
                .keyword
                .as_ref()
                .map(|k| k.keyword())
                .unwrap_or("-")
        ));
        out.push_str(&format!("title: {}\n", headline_title_text(headline)));
        out.push_str(&format!(
            "id: {}\n",
            headline_id(headline).unwrap_or_else(|| "-".to_string())
        ));
        if include_subtree {
            out.push_str("\ncontent:\n");
            out.push_str(&format!("{headline}"));
        }

        org_tool_ok(&tool_use.id, out)
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// FindHeadlinesTool ////////////////////////////////////

struct FindHeadlinesTool {
    document: SharedDocument,
}

impl FindHeadlinesTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for FindHeadlinesTool {
    fn name(&self) -> String {
        "find_headlines".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(FindHeadlinesCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "find_headlines".to_string(),
            description: Some("Find headlines by title substring.".to_string()),
            input_schema: json_schema(
                &[
                    (
                        "query",
                        "Case-insensitive substring to match in headline titles",
                        serde_json::json!({"type": "string"}),
                    ),
                    (
                        "limit",
                        "Maximum number of matches to return (default 20)",
                        serde_json::json!({"type": "integer"}),
                    ),
                ],
                &["query"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct FindHeadlinesCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for FindHeadlinesCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let query = match get_required_str(tool_use, "query") {
            Ok(query) => query.to_lowercase(),
            Err(err) => return org_tool_err(&tool_use.id, err),
        };
        let limit = get_optional_u64(tool_use, "limit").unwrap_or(20).max(1) as usize;

        let doc = self.document.lock().unwrap();
        let mut rows = Vec::new();
        collect_headline_rows(&doc.headlines, &mut Vec::new(), &mut rows);

        let mut out = Vec::new();
        for (path, state, title, id, _level) in rows {
            if title.to_lowercase().contains(&query) {
                out.push(format!(
                    "{path} | {state} | {} | {title}",
                    id.unwrap_or_else(|| "-".to_string())
                ));
                if out.len() >= limit {
                    break;
                }
            }
        }

        if out.is_empty() {
            org_tool_ok(&tool_use.id, "No matching headlines.")
        } else {
            org_tool_ok(&tool_use.id, out.join("\n"))
        }
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// DeleteRangeTool //////////////////////////////////////

struct DeleteRangeTool {
    document: SharedDocument,
}

impl DeleteRangeTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for DeleteRangeTool {
    fn name(&self) -> String {
        "delete_range".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(DeleteRangeCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "delete_range".to_string(),
            description: Some(
                "Delete a top-level headline range (inclusive) to clean up duplicates quickly."
                    .to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "start",
                        "Start top-level index (inclusive)",
                        serde_json::json!({"type": "integer"}),
                    ),
                    (
                        "end",
                        "End top-level index (inclusive)",
                        serde_json::json!({"type": "integer"}),
                    ),
                    (
                        "dry_run",
                        "If true, only preview which top-level headlines would be removed",
                        serde_json::json!({"type": "boolean"}),
                    ),
                ],
                &["start", "end"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct DeleteRangeCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for DeleteRangeCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let start = match get_required_u64(tool_use, "start") {
            Ok(value) => value as usize,
            Err(err) => return org_tool_err(&tool_use.id, err),
        };
        let end = match get_required_u64(tool_use, "end") {
            Ok(value) => value as usize,
            Err(err) => return org_tool_err(&tool_use.id, err),
        };
        if start > end {
            return org_tool_err(&tool_use.id, "start must be <= end");
        }
        let dry_run = get_optional_bool(tool_use, "dry_run").unwrap_or(false);

        let mut doc = self.document.lock().unwrap();
        if end >= doc.headlines.len() {
            return org_tool_err(
                &tool_use.id,
                format!(
                    "range [{start}, {end}] is out of bounds for {} top-level headlines",
                    doc.headlines.len()
                ),
            );
        }

        let mut preview = Vec::new();
        for idx in start..=end {
            let headline = &doc.headlines[idx];
            preview.push(format!("[{idx}] {}", headline_title_text(headline)));
        }

        if dry_run {
            return org_tool_ok(
                &tool_use.id,
                format!(
                    "Dry run: would delete {} top-level headlines:\n{}",
                    preview.len(),
                    preview.join("\n")
                ),
            );
        }

        doc.headlines.drain(start..(end + 1));
        org_tool_ok(
            &tool_use.id,
            format!(
                "Deleted {} top-level headlines:\n{}",
                preview.len(),
                preview.join("\n")
            ),
        )
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// ClearDocumentTool ////////////////////////////////////

struct ClearDocumentTool {
    document: SharedDocument,
}

impl ClearDocumentTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for ClearDocumentTool {
    fn name(&self) -> String {
        "clear_document".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(ClearDocumentCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "clear_document".to_string(),
            description: Some(
                "Clear the in-memory document. Requires confirm=true for safety.".to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "confirm",
                        "Must be true to perform the clear operation",
                        serde_json::json!({"type": "boolean"}),
                    ),
                    (
                        "dry_run",
                        "If true, only report what would be removed",
                        serde_json::json!({"type": "boolean"}),
                    ),
                ],
                &["confirm"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct ClearDocumentCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for ClearDocumentCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let confirm = get_optional_bool(tool_use, "confirm").unwrap_or(false);
        if !confirm {
            return org_tool_err(&tool_use.id, "clear_document requires confirm=true");
        }
        let dry_run = get_optional_bool(tool_use, "dry_run").unwrap_or(false);

        let mut doc = self.document.lock().unwrap();
        let removed_top_level = doc.headlines.len();
        let had_zeroth = doc.zeroth_section.is_some();
        if dry_run {
            return org_tool_ok(
                &tool_use.id,
                format!(
                    "Dry run: would clear document (top-level headlines: {removed_top_level}, zeroth section: {}).",
                    if had_zeroth { "present" } else { "absent" }
                ),
            );
        }

        *doc = Document::default();
        org_tool_ok(
            &tool_use.id,
            format!("Cleared document (removed {removed_top_level} top-level headlines)."),
        )
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// ValidateDocumentTool /////////////////////////////////

struct ValidateDocumentTool {
    document: SharedDocument,
}

impl ValidateDocumentTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for ValidateDocumentTool {
    fn name(&self) -> String {
        "validate_document".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(ValidateDocumentCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "validate_document".to_string(),
            description: Some(
                "Validate document parseability and report duplicate IDs.".to_string(),
            ),
            input_schema: json_schema(&[], &[]),
            cache_control: None,
            strict: None,
        })
    }
}

struct ValidateDocumentCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for ValidateDocumentCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let doc = self.document.lock().unwrap();
        let text = format!("{doc}");
        let mut report = Vec::new();

        match orgremode::parse(&text) {
            Ok(_) => report.push("parse: ok".to_string()),
            Err(err) => report.push(format!("parse: error ({err})")),
        }

        let mut duplicates = Vec::new();
        collect_duplicate_ids(&doc.headlines, &mut duplicates);
        if duplicates.is_empty() {
            report.push("duplicate IDs: none".to_string());
        } else {
            report.push(format!("duplicate IDs: {}", duplicates.join(", ")));
        }

        if report
            .iter()
            .all(|line| line.ends_with("ok") || line.ends_with("none"))
        {
            report.insert(0, "validation: clean".to_string());
        } else {
            report.insert(0, "validation: issues found".to_string());
        }

        org_tool_ok(&tool_use.id, report.join("\n"))
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// AskQuestionsTool /////////////////////////////////////

/// Tool that allows the agent to ask the user a series of questions.
struct AskQuestionsTool;

impl<A: Agent> Tool<A> for AskQuestionsTool {
    fn name(&self) -> String {
        "ask_questions".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(AskQuestionsCallback)
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "ask_questions".to_string(),
            description: Some(
                "Ask the user a series of questions one at a time. Returns the answers."
                    .to_string(),
            ),
            input_schema: json_schema(
                &[(
                    "questions",
                    "Array of questions to ask the user",
                    serde_json::json!({"type": "array", "items": {"type": "string"}}),
                )],
                &["questions"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct AskQuestionsCallback;

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for AskQuestionsCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let questions = match get_required_string_array(tool_use, "questions") {
            Ok(questions) => questions,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };

        if questions.is_empty() {
            return org_tool_err(&tool_use.id, "No questions provided");
        }

        let mut answers: Vec<(String, String)> = Vec::new();

        for question in questions {
            println!("\n[Question from agent]: {}", question);
            match read_user_input() {
                Ok(Some(answer)) => {
                    answers.push((question, answer));
                }
                Ok(None) => {
                    return org_tool_err(&tool_use.id, "End of input while answering questions");
                }
                Err(e) => {
                    return org_tool_err(&tool_use.id, format!("Error reading input: {}", e));
                }
            }
        }

        let response = answers
            .iter()
            .map(|(q, a)| format!("Q: {}\nA: {}", q, a))
            .collect::<Vec<_>>()
            .join("\n\n");

        org_tool_ok(&tool_use.id, response)
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

//////////////////////////////////////////// EnsureIdTool /////////////////////////////////////////

struct EnsureIdTool {
    document: SharedDocument,
}

impl EnsureIdTool {
    fn new(document: SharedDocument) -> Self {
        Self { document }
    }
}

impl<A: Agent> Tool<A> for EnsureIdTool {
    fn name(&self) -> String {
        "ensure_id".to_string()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(EnsureIdCallback {
            document: Arc::clone(&self.document),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(ToolParam {
            name: "ensure_id".to_string(),
            description: Some(
                "Ensure a headline at the given path has an ID property. Returns the ID."
                    .to_string(),
            ),
            input_schema: json_schema(
                &[
                    (
                        "path",
                        "Path to the headline as array of indices",
                        serde_json::json!({"type": "array", "items": {"type": "integer"}}),
                    ),
                    (
                        "preferred_id",
                        "Preferred ID to use if none exists",
                        serde_json::json!({"type": ["string", "null"]}),
                    ),
                ],
                &["path"],
            ),
            cache_control: None,
            strict: None,
        })
    }
}

struct EnsureIdCallback {
    document: SharedDocument,
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for EnsureIdCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        let path = match get_required_usize_array(tool_use, "path") {
            Ok(path) => path,
            Err(e) => return org_tool_err(&tool_use.id, e),
        };
        let preferred_id = get_optional_str(tool_use, "preferred_id");

        let mut doc = self.document.lock().unwrap();
        let result = orgremode::ensure_id(&mut doc, &NodePath(path), preferred_id)
            .map(|id| format!("ID: {}", id))
            .map_err(|e| e.to_string());

        Box::new(OrgToolResult {
            tool_use_id: tool_use.id.clone(),
            result,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        apply_org_tool_result(intermediate)
    }
}

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

#[derive(Debug, Clone)]
enum Command {
    Show,
    Outline,
    Focus(String),
    Back,
    Diff,
    Undo,
    Redo,
    Help,
    Git(String),
    Quit,
    Unknown(String),
}

fn parse_command(input: &str) -> Option<Command> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or_default().to_lowercase();
    let rest = parts
        .next()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let command = match cmd.as_str() {
        "/show" | "/doc" => Command::Show,
        "/outline" => Command::Outline,
        "/focus" | "/goto" => {
            if rest.is_empty() {
                Command::Unknown("missing target for /focus".to_string())
            } else {
                Command::Focus(rest)
            }
        }
        "/back" => Command::Back,
        "/diff" => Command::Diff,
        "/status" => Command::Git("status".to_string()),
        "/git" => {
            if rest.is_empty() {
                Command::Unknown("missing args for /git".to_string())
            } else {
                Command::Git(rest)
            }
        }
        "/undo" => Command::Undo,
        "/redo" => Command::Redo,
        "/help" => Command::Help,
        "/quit" | "/exit" => Command::Quit,
        other => Command::Unknown(format!("unknown command: {other}")),
    };
    Some(command)
}

fn parse_node_path(input: &str) -> Option<NodePath> {
    let trimmed = input.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    if inner.trim().is_empty() {
        return None;
    }
    let mut path = Vec::new();
    for part in inner.split(',') {
        let value = part.trim().parse::<usize>().ok()?;
        path.push(value);
    }
    Some(NodePath(path))
}

fn headline_by_path<'a>(headlines: &'a [Headline], path: &[usize]) -> Option<&'a Headline> {
    let (first, rest) = path.split_first()?;
    let headline = headlines.get(*first)?;
    if rest.is_empty() {
        Some(headline)
    } else {
        headline_by_path(&headline.children, rest)
    }
}

fn headline_title_text(headline: &Headline) -> String {
    let mut text = String::new();
    for obj in &headline.title {
        text.push_str(&format!("{obj}"));
    }
    text.trim().to_string()
}

fn headline_id(headline: &Headline) -> Option<String> {
    let props = headline.properties.as_ref()?;
    for prop in &props.properties {
        if prop.name().eq_ignore_ascii_case("ID") {
            return Some(prop.value().to_string());
        }
    }
    None
}

fn format_node_path(path: &[usize]) -> String {
    let mut out = String::from("[");
    for (idx, entry) in path.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&entry.to_string());
    }
    out.push(']');
    out
}

fn resolve_headline_path_by_id_or_path(
    doc: &Document,
    id: Option<&str>,
    path: Option<&[usize]>,
) -> Result<NodePath, String> {
    match (id, path) {
        (Some(id), None) => orgremode::find_node_path_by_id(doc, id)
            .ok_or_else(|| format!("node not found for ID '{id}'")),
        (None, Some(path)) => {
            if headline_by_path(&doc.headlines, path).is_some() {
                Ok(NodePath(path.to_vec()))
            } else {
                Err(format!("no headline at path {}", format_node_path(path)))
            }
        }
        (Some(_), Some(_)) => Err("provide either 'id' or 'path', not both".to_string()),
        (None, None) => Err("missing selector: provide 'id' or 'path'".to_string()),
    }
}

fn collect_headline_rows(
    headlines: &[Headline],
    path: &mut Vec<usize>,
    rows: &mut Vec<(String, String, String, Option<String>, usize)>,
) {
    for (idx, headline) in headlines.iter().enumerate() {
        path.push(idx);
        let path_text = format_node_path(path);
        let state = headline
            .keyword
            .as_ref()
            .map(|k| k.keyword().to_string())
            .unwrap_or_else(|| "-".to_string());
        let title = headline_title_text(headline);
        let id = headline_id(headline);
        rows.push((path_text, state, title, id, headline.level as usize));
        if !headline.children.is_empty() {
            collect_headline_rows(&headline.children, path, rows);
        }
        path.pop();
    }
}

fn collect_duplicate_ids(headlines: &[Headline], duplicates: &mut Vec<String>) {
    use std::collections::HashSet;
    fn walk(headlines: &[Headline], seen: &mut HashSet<String>, duplicates: &mut Vec<String>) {
        for headline in headlines {
            if let Some(id) = headline_id(headline)
                && !seen.insert(id.clone())
                && !duplicates.iter().any(|dup| dup == &id)
            {
                duplicates.push(id);
            }
            if !headline.children.is_empty() {
                walk(&headline.children, seen, duplicates);
            }
        }
    }
    let mut seen = HashSet::new();
    walk(headlines, &mut seen, duplicates);
}

fn resolve_headline_path(doc: &Document, target: &str) -> Result<NodePath, String> {
    if let Some(path) = parse_node_path(target) {
        if headline_by_path(&doc.headlines, &path.0).is_some() {
            return Ok(path);
        }
        return Err(format!("no headline at path {target}"));
    }
    if let Some(path) = orgremode::find_node_path_by_id(doc, target) {
        return Ok(path);
    }
    let needle = target.to_lowercase();
    let mut best: Option<NodePath> = None;
    let mut stack: Vec<(NodePath, &Headline)> = doc
        .headlines
        .iter()
        .enumerate()
        .map(|(idx, h)| (NodePath(vec![idx]), h))
        .collect();
    while let Some((path, headline)) = stack.pop() {
        let title = headline_title_text(headline).to_lowercase();
        if title == needle {
            return Ok(path);
        }
        if best.is_none() && title.contains(&needle) {
            best = Some(path.clone());
        }
        for (child_idx, child) in headline.children.iter().enumerate() {
            let mut child_path = path.0.clone();
            child_path.push(child_idx);
            stack.push((NodePath(child_path), child));
        }
    }
    best.ok_or_else(|| format!("no headline matches \"{target}\""))
}

fn render_document(doc: &Document, focus: &Option<NodePath>) -> String {
    if let Some(path) = focus {
        if let Some(headline) = headline_by_path(&doc.headlines, &path.0) {
            return format!("{headline}");
        }
        return "Focus path no longer exists. Use /back.".to_string();
    }
    format!("{doc}")
}

fn render_outline(doc: &Document) -> String {
    let mut lines = Vec::new();
    fn walk(headlines: &[Headline], lines: &mut Vec<String>) {
        for headline in headlines {
            let level = headline.level.max(1) as usize;
            let stars = "*".repeat(level);
            let mut line = format!("{stars} ");
            if let Some(keyword) = &headline.keyword {
                line.push_str(keyword.keyword());
                line.push(' ');
            }
            if let Some(priority) = &headline.priority {
                line.push_str(&format!("[#{}] ", priority.value()));
            }
            let title = headline_title_text(headline);
            line.push_str(&title);
            if !headline.tags.is_empty() {
                line.push(' ');
                line.push(':');
                for tag in &headline.tags {
                    line.push_str(tag.tag());
                    line.push(':');
                }
            }
            if let Some(id) = headline_id(headline) {
                line.push_str(&format!(" (ID: {id})"));
            }
            lines.push(line);
            if !headline.children.is_empty() {
                walk(&headline.children, lines);
            }
        }
    }
    walk(&doc.headlines, &mut lines);
    if lines.is_empty() {
        "No headlines.".to_string()
    } else {
        lines.join("\n")
    }
}

fn apply_snapshot(
    document: &SharedDocument,
    snapshot: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let doc = orgremode::parse(snapshot)?;
    let mut guard = document.lock().unwrap();
    *guard = doc;
    Ok(())
}

fn save_document(document: &SharedDocument, output_file: &str) -> Result<(), std::io::Error> {
    let doc = document.lock().unwrap();
    let content = format!("{doc}");
    std::fs::write(output_file, content)
}

fn run_git_command(args: &str) {
    let argv = match shvar::split(args) {
        Ok(parts) => parts,
        Err(err) => {
            println!("Failed to parse git args: {err:?}");
            return;
        }
    };
    if argv.is_empty() {
        println!("Missing git args.");
        return;
    }
    let mut cmd = std::process::Command::new("git");
    cmd.args(&argv);
    match cmd.output() {
        Ok(output) => {
            if !output.stdout.is_empty() {
                print!("{}", String::from_utf8_lossy(&output.stdout));
            }
            if !output.stderr.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
            }
            if !output.status.success() {
                println!("git exited with status: {}", output.status);
            }
        }
        Err(err) => {
            println!("Failed to run git: {err}");
        }
    }
}

fn handle_command(
    command: Command,
    state: &mut SessionState,
    document: &SharedDocument,
    output_file: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::Show => {
            let doc = document.lock().unwrap();
            let rendered = render_document(&doc, &state.focus);
            if rendered.trim().is_empty() {
                println!("(empty document)");
            } else {
                println!("{rendered}");
            }
        }
        Command::Outline => {
            let doc = document.lock().unwrap();
            println!("{}", render_outline(&doc));
        }
        Command::Focus(target) => {
            let doc = document.lock().unwrap();
            match resolve_headline_path(&doc, &target) {
                Ok(path) => {
                    state.focus = Some(path);
                    println!("Focus set. Use /show to view or /back to clear.");
                }
                Err(err) => {
                    println!("Focus failed: {err}");
                }
            }
        }
        Command::Back => {
            state.focus = None;
            println!("Focus cleared.");
        }
        Command::Diff => {
            if let Err(e) = save_document(document, output_file) {
                eprintln!("Failed to save before git diff: {e}");
            }
            run_git_command("diff");
        }
        Command::Undo => {
            if let Some(snapshot) = state.undo_stack.pop() {
                let current = {
                    let doc = document.lock().unwrap();
                    format!("{doc}")
                };
                state.redo_stack.push(current);
                if let Err(err) = apply_snapshot(document, &snapshot) {
                    println!("Undo failed: {err}");
                } else {
                    println!("Undid last change.");
                }
            } else {
                println!("Nothing to undo.");
            }
        }
        Command::Redo => {
            if let Some(snapshot) = state.redo_stack.pop() {
                let current = {
                    let doc = document.lock().unwrap();
                    format!("{doc}")
                };
                state.undo_stack.push(current);
                if let Err(err) = apply_snapshot(document, &snapshot) {
                    println!("Redo failed: {err}");
                } else {
                    println!("Redid last change.");
                }
            } else {
                println!("Nothing to redo.");
            }
        }
        Command::Help => {
            println!("{HELP_TEXT}");
        }
        Command::Git(args) => {
            if let Err(e) = save_document(document, output_file) {
                eprintln!("Failed to save before git: {e}");
            }
            run_git_command(&args);
        }
        Command::Quit => return Ok(false),
        Command::Unknown(message) => {
            println!("{message}");
            println!("Type /help for commands.");
        }
    }
    Ok(true)
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

async fn run_with_config(
    config: &PlanningSessionConfig,
    output_file: &str,
    filesystem_root: Option<&str>,
    skills_root: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(None)?;
    let budget = Arc::new(Budget::new_with_rates(1_000_000_000, 500, 2500, 625, 50));

    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_clone = interrupted.clone();
    ctrlc::set_handler(move || {
        interrupted_clone.store(true, Ordering::Relaxed);
    })?;

    let (document, mut session_state): (SharedDocument, SessionState) = if std::path::Path::new(
        output_file,
    )
    .exists()
    {
        let content = std::fs::read_to_string(output_file)
            .map_err(|e| format!("failed to read {output_file}: {e}"))?;
        let doc = orgremode::parse(&content).map_err(|e| {
                format!(
                    "failed to parse {output_file}: {e}\nhint: fix the org syntax or start fresh with an empty file"
                )
            })?;
        (Arc::new(Mutex::new(doc)), SessionState::new())
    } else {
        (
            Arc::new(Mutex::new(Document::default())),
            SessionState::new(),
        )
    };

    // Convert filesystem root to absolute path if provided
    let filesystem_root: Option<Path<'static>> = if let Some(root) = filesystem_root {
        let abs_path = std::fs::canonicalize(root)?;
        let path_str = abs_path
            .to_str()
            .ok_or("filesystem root path is not valid UTF-8")?;
        Some(Path::from(path_str).into_owned())
    } else {
        None
    };

    // Convert skills root to absolute path if provided
    let skills_root: Option<Path<'static>> = if let Some(root) = skills_root {
        let abs_path = std::fs::canonicalize(root)?;
        let path_str = abs_path
            .to_str()
            .ok_or("skills root path is not valid UTF-8")?;
        Some(Path::from(path_str).into_owned())
    } else {
        None
    };

    let mutation_counter = Arc::new(MutationCounter::new(
        config.mutation_cap.unwrap_or(DEFAULT_MUTATION_CAP),
    ));
    let mut messages: Vec<MessageParam> = Vec::new();

    if !config.oneshot {
        println!("=== Planning Session ===");
        println!("Output file: {}", output_file);
        if let Some(ref root) = filesystem_root {
            println!("Filesystem: / -> {} (read-only)", root);
        }
        if let Some(ref root) = skills_root {
            println!("Skills: /.skills -> {} (read-only)", root);
        }
        println!("Chat with the assistant to plan your work.");
        println!("Type '/help' for commands.");
        println!("Type 'quit' or 'exit' to end the session.");
        println!("Press Ctrl+C to interrupt a response.\n");
    }

    if config.oneshot {
        messages.push(create_user_message("Proceed with the one-shot request."));

        mutation_counter.reset();
        let agent =
            PlanningSessionAgent::new(config, Arc::clone(&document), Arc::clone(&mutation_counter));
        let mut agent = agent.with_filesystem(filesystem_root.clone(), skills_root.clone());

        let mut renderer = OneShotTextOnlyRenderer::new(interrupted.clone());
        let context = ();

        let result = agent
            .take_turn_streaming_root(&client, &mut messages, &budget, &mut renderer)
            .await;

        match result {
            Ok(_outcome) => {
                renderer.finish_response(&context);
            }
            Err(e) => {
                renderer.finish_response(&context);
                eprintln!("\noneshot: agent execution failed: {e}");
                if let Some(tool_report) = renderer.format_tool_errors() {
                    eprint!("{tool_report}");
                }
                eprintln!("hint: refine your --oneshot prompt and retry");
                std::process::exit(1);
            }
        }

        if let Some(tool_report) = renderer.format_tool_errors() {
            eprint!("{tool_report}");
        }

        let doc = document.lock().unwrap();
        let content = format!("{doc}");
        if config.dry_run {
            eprintln!("\n--- dry-run: document not written ---");
            eprint!("{content}");
            eprintln!("--- end dry-run ---");
        } else {
            std::fs::write(output_file, &content)?;
        }
        return Ok(());
    }

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
            let keep_going = handle_command(command, &mut session_state, &document, output_file)?;
            if !keep_going {
                println!("\nEnding session.");
                break;
            }
            continue;
        }

        let pre_turn = {
            let doc = document.lock().unwrap();
            format!("{doc}")
        };
        messages.push(create_user_message(&input));

        mutation_counter.reset();
        let agent =
            PlanningSessionAgent::new(config, Arc::clone(&document), Arc::clone(&mutation_counter));
        let mut agent = agent.with_filesystem(filesystem_root.clone(), skills_root.clone());

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

        let post_turn = {
            let doc = document.lock().unwrap();
            format!("{doc}")
        };
        if post_turn != pre_turn {
            let mutations = mutation_counter.count.load(Ordering::Relaxed);
            println!(
                "\n[{} mutation{} this turn. Type /undo to revert or Enter to keep.]",
                mutations,
                if mutations == 1 { "" } else { "s" },
            );
            session_state.undo_stack.push(pre_turn);
            session_state.redo_stack.clear();
            if let Err(e) = save_document(&document, output_file) {
                eprintln!("Failed to auto-save: {e}");
            }
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!("Usage: orgremode --skill");
    eprintln!("   or: orgremode [OPTIONS] <output.org> [filesystem-root] [skills-root]");
    eprintln!();
    eprintln!("Interactive planning session that builds an org-mode file.");
    eprintln!("The agent has tools to manipulate the document directly.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --skill            Print a SKILL.md template for orgremode --oneshot and exit");
    eprintln!("  --oneshot TEXT     One-shot mode with seed prompt: do it all or return an error");
    eprintln!("  --dry-run          Preview agent output without writing the file (oneshot only)");
    eprintln!("  --mutation-cap N   Override the default mutation cap per turn");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  output.org         Path to the org-mode file to edit");
    eprintln!("  filesystem-root    Optional path to mount at / (read-only)");
    eprintln!("  skills-root        Optional path to mount at /.skills (read-only)");
    eprintln!();
}

fn readme() -> String {
    SKILL_HEADER.to_string()
        + include_str!("../../README.md")
            .split_once("\n## Non-Skill Information")
            .expect("README.md must have Non-Skill Information section")
            .0
            .trim()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (options, free) = CliOptions::from_command_line_relaxed(
        "Usage: orgremode --skill | orgremode [--oneshot TEXT] <output.org> [filesystem-root] [skills-root]",
    );

    if options.skill {
        println!("{}", readme());
        return Ok(());
    }

    if free.is_empty() || free.len() > 3 {
        print_usage();
        std::process::exit(1);
    }

    let output_file = free[0].as_str();
    let filesystem_root = free.get(1).map(|s| s.as_str());
    let skills_root = free.get(2).map(|s| s.as_str());

    let mut config = PlanningSessionConfig::default();
    if let Some(seed_prompt) = options.oneshot.as_ref() {
        config.oneshot = true;
        config.system_prompt = format!(
            "{}\n\n{}\n\n<one-shot-seed>\n{}\n</one-shot-seed>",
            config.system_prompt.trim_end(),
            ONESHOT_SYSTEM_PROMPT,
            seed_prompt.trim()
        );
    }
    if options.dry_run {
        if !config.oneshot {
            eprintln!("--dry-run requires --oneshot");
            std::process::exit(1);
        }
        config.dry_run = true;
    }
    if let Some(cap) = options.mutation_cap {
        config.mutation_cap = Some(cap);
    }
    run_with_config(&config, output_file, filesystem_root, skills_root).await
}
