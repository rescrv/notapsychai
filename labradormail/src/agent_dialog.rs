//! Agent chat dialog implementation.
//!
//! Provides a persistent chat dialog for interacting with Claude AI.

use std::any::Any;
use std::io::Result;
use std::io::Write;
use std::ops::ControlFlow;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use agent_inbox_protocol::MessageID;
use agent_inbox_protocol::ToolCallRequest;
use claudius::chat::ChatAgent;
use claudius::chat::ChatConfig;
use claudius::chat::ChatSession;
use claudius::push_or_merge_message;
use claudius::Agent;
use claudius::Anthropic;
use claudius::ContentBlock;
use claudius::DocumentBlock;
use claudius::DocumentSource;
use claudius::IntermediateToolResult;
use claudius::KnownModel;
use claudius::MessageParam;
use claudius::MessageParamContent;
use claudius::MessageRole;
use claudius::Model;
use claudius::PlainTextSource;
use claudius::Renderer;
use claudius::StopReason;
use claudius::StreamContext;
use claudius::SystemPrompt;
use claudius::TextBlock;
use claudius::ThinkingConfig;
use claudius::Tool;
use claudius::ToolCallback;
use claudius::ToolParam;
use claudius::ToolResult;
use claudius::ToolResultBlock;
use claudius::ToolResultBlockContent;
use claudius::ToolUnionParam;
use claudius::ToolUseBlock;
use serde_json::Map;
use serde_json::Value;

use crate::context::ColorId;
use crate::context::GuiContext;
use crate::curs_lib::pad_string;
use crate::curs_lib::ScrollState;
use crate::help_data::HelpData;
use crate::help_data::HelpItem;
use crate::inbox::tool_display_name;
use crate::inbox::ToolOrigin;
use crate::render::Renderer as UiRenderer;
use crate::sbar::StatusBar;
use crate::window::CursorBehavior;
use crate::window::CursorState;
use crate::window::RenderMode;
use crate::window::WindowId;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowTree;
use crate::window::WindowType;
use crate::window::WindowWidget;
use std::collections::HashSet;

static SPINNERS: &[(u8, &[char])] = &[
    (8, &['|', '/', '-', '\\']),
    (12, &['.', 'o', 'O', 'o']),
    (10, &['-', '\\', '|', '/']),
];

//////////////////////////////////////////// DynamicTool ///////////////////////////////////////////

/// A tool dynamically loaded from a yanked message.
///
/// Wraps a `ToolParam` from an agent-inbox-protocol message and provides
/// the necessary information to call the tool via HTTP.
#[derive(Clone)]
struct DynamicTool {
    /// The tool parameter definition.
    param: ToolParam,
    /// The tool name expected by the server.
    original_name: String,
    /// Whether the tool expects an object input.
    expects_object: bool,
    /// The message ID this tool belongs to.
    message_id: MessageID,
    /// The base URL of the server that handles this tool.
    base_url: String,
}

impl DynamicTool {
    fn new(
        param: ToolParam,
        original_name: String,
        expects_object: bool,
        message_id: MessageID,
        base_url: String,
    ) -> Self {
        Self {
            param,
            original_name,
            expects_object,
            message_id,
            base_url,
        }
    }
}

impl<A: Agent> Tool<A> for DynamicTool {
    fn name(&self) -> String {
        self.param.name.clone()
    }

    fn callback(&self) -> Box<dyn ToolCallback<A> + '_> {
        Box::new(DynamicToolCallback {
            message_id: self.message_id.clone(),
            tool_name: self.original_name.clone(),
            expects_object: self.expects_object,
            base_url: self.base_url.clone(),
        })
    }

    fn to_param(&self) -> ToolUnionParam {
        ToolUnionParam::CustomTool(self.param.clone())
    }
}

/// Callback for executing dynamic tools via agent-inbox-protocol.
struct DynamicToolCallback {
    message_id: MessageID,
    tool_name: String,
    expects_object: bool,
    base_url: String,
}

fn schema_expects_object(schema: &Value) -> bool {
    match schema {
        Value::Object(map) => {
            if map
                .get("type")
                .and_then(|value| value.as_str())
                .is_some_and(|value| value == "object")
            {
                return true;
            }
            if let Some(inner) = map.get("schema").or_else(|| map.get("input_schema")) {
                return schema_expects_object(inner);
            }
            false
        }
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .is_some_and(|parsed| schema_expects_object(&parsed)),
        _ => false,
    }
}

fn tool_name_tag(message_id: &MessageID) -> String {
    let raw = format!("{message_id:?}");
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in raw.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:x}", hash)
}

fn build_tool_name(base_name: &str, suffix: &str) -> String {
    let base_len = base_name.len();
    let suffix_len = suffix.len();
    if base_len + 2 + suffix_len <= 128 {
        return format!("{}__{}", base_name, suffix);
    }
    let max_base = 128usize.saturating_sub(2 + suffix_len);
    let trimmed_base = if max_base == 0 {
        ""
    } else if base_len <= max_base {
        base_name
    } else {
        &base_name[..max_base]
    };
    format!("{}__{}", trimmed_base, suffix)
}

fn unique_tool_name(
    base_name: &str,
    message_id: &MessageID,
    used_names: &mut HashSet<String>,
) -> String {
    let message_id_tag = tool_name_tag(message_id);
    let mut candidate = build_tool_name(base_name, &message_id_tag);
    if used_names.insert(candidate.clone()) {
        return candidate;
    }
    let mut counter = 2;
    loop {
        let suffix = format!("{}__{}", message_id_tag, counter);
        candidate = build_tool_name(base_name, &suffix);
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        counter += 1;
    }
}

fn normalize_tool_input(input: &Value, expects_object: bool) -> Value {
    if !expects_object {
        return input.clone();
    }
    match input {
        Value::Object(_) => input.clone(),
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .filter(|parsed| parsed.is_object())
            .unwrap_or_else(|| Value::Object(Map::new())),
        Value::Null => Value::Object(Map::new()),
        _ => Value::Object(Map::new()),
    }
}

#[async_trait::async_trait]
impl<A: Agent> ToolCallback<A> for DynamicToolCallback {
    async fn compute_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &A,
        tool_use: &ToolUseBlock,
    ) -> Box<dyn IntermediateToolResult> {
        // Make the HTTP call to the agent-inbox-protocol server
        let input = normalize_tool_input(&tool_use.input, self.expects_object);
        let request = ToolCallRequest {
            message_id: self.message_id.clone(),
            name: self.tool_name.clone(),
            input,
        };

        let http_client = reqwest::Client::new();
        let url = format!("{}/call", self.base_url.trim_end_matches('/'));
        let result: std::result::Result<reqwest::Response, reqwest::Error> =
            http_client.post(&url).json(&request).send().await;

        let response = match result {
            Ok(resp) => match resp.json::<agent_inbox_protocol::ToolCallResponse>().await {
                Ok(r) => r,
                Err(e) => agent_inbox_protocol::ToolCallResponse::failure(format!(
                    "Failed to parse response: {}",
                    e
                )),
            },
            Err(e) => agent_inbox_protocol::ToolCallResponse::failure(format!("HTTP error: {}", e)),
        };

        Box::new(DynamicToolIntermediateResult {
            tool_use_id: tool_use.id.clone(),
            response,
        })
    }

    async fn apply_tool_result(
        &self,
        _client: &Anthropic,
        _agent: &mut A,
        _tool_use: &ToolUseBlock,
        intermediate: Box<dyn IntermediateToolResult>,
    ) -> ToolResult {
        let result = intermediate
            .as_any()
            .downcast_ref::<DynamicToolIntermediateResult>()
            .expect("intermediate result type mismatch");

        let content = result
            .response
            .message
            .clone()
            .unwrap_or_else(|| "Tool executed".to_string());

        let block = ToolResultBlock {
            tool_use_id: result.tool_use_id.clone(),
            content: Some(ToolResultBlockContent::String(content)),
            is_error: Some(!result.response.success),
            cache_control: None,
        };

        if result.response.success {
            ControlFlow::Continue(Ok(block))
        } else {
            ControlFlow::Continue(Err(block))
        }
    }
}

/// Intermediate result from a dynamic tool call.
struct DynamicToolIntermediateResult {
    tool_use_id: String,
    response: agent_inbox_protocol::ToolCallResponse,
}

impl IntermediateToolResult for DynamicToolIntermediateResult {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

//////////////////////////////////////// DynamicToolsAgent /////////////////////////////////////////

/// Shared storage for dynamic tools.
type SharedDynamicTools = Arc<RwLock<Vec<Arc<DynamicTool>>>>;

/// An agent that wraps ChatConfig and adds dynamic tools from yanked messages.
pub struct DynamicToolsAgent {
    config: ChatConfig,
    dynamic_tools: SharedDynamicTools,
}

impl DynamicToolsAgent {
    /// Creates a new agent with the given config and shared tool storage.
    fn new(config: ChatConfig, dynamic_tools: SharedDynamicTools) -> Self {
        Self {
            config,
            dynamic_tools,
        }
    }
}

#[async_trait::async_trait]
impl Agent for DynamicToolsAgent {
    async fn max_tokens(&self) -> u32 {
        self.config.max_tokens()
    }

    async fn model(&self) -> Model {
        self.config.model()
    }

    async fn stop_sequences(&self) -> Option<Vec<String>> {
        let sequences = self.config.stop_sequences();
        if sequences.is_empty() {
            None
        } else {
            Some(sequences.to_vec())
        }
    }

    async fn system(&self) -> Option<SystemPrompt> {
        self.config.template.system.clone()
    }

    async fn temperature(&self) -> Option<f32> {
        self.config.template.temperature
    }

    async fn thinking(&self) -> Option<ThinkingConfig> {
        self.config.template.thinking
    }

    async fn top_k(&self) -> Option<u32> {
        self.config.template.top_k
    }

    async fn top_p(&self) -> Option<f32> {
        self.config.template.top_p
    }

    async fn tools(&self) -> Vec<Arc<dyn Tool<Self>>> {
        let tools = self.dynamic_tools.read().unwrap();
        tools
            .iter()
            .map(|t| Arc::clone(t) as Arc<dyn Tool<Self>>)
            .collect()
    }
}

impl ChatAgent for DynamicToolsAgent {
    fn config(&self) -> &ChatConfig {
        &self.config
    }

    fn config_mut(&mut self) -> &mut ChatConfig {
        &mut self.config
    }
}

/// Shared state for streaming responses.
#[derive(Debug)]
pub struct StreamingState {
    /// Accumulated text from the stream.
    pub text: String,
    /// Whether streaming is complete.
    pub complete: bool,
    /// Error message if streaming failed.
    pub error: Option<String>,
    /// Flag to signal cancellation.
    pub cancelled: Arc<AtomicBool>,
}

impl StreamingState {
    fn new() -> Self {
        Self {
            text: String::new(),
            complete: false,
            error: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Shared streaming state wrapped in Arc<Mutex>.
pub type SharedStreamingState = Arc<Mutex<StreamingState>>;

/// Receiver type for async chat completion.
type ChatCompletionReceiver = Receiver<(ChatSession<DynamicToolsAgent>, Result<()>)>;

/// Mutable state for the agent dialog content.
pub struct AgentDialogContentData {
    /// User input buffer.
    input: String,
    /// Cursor position within input.
    cursor_pos: usize,
    /// Display messages for rendering.
    messages: Vec<MessageParam>,
    /// Scroll state for message area.
    scroll: ScrollState,
    /// Formatted lines for display (cached).
    formatted_lines: Vec<FormattedLine>,
    /// Last known width (for reformatting).
    last_width: i16,
    /// Whether we're currently streaming a response.
    streaming: bool,
    /// Shared streaming state for async updates.
    streaming_state: Option<SharedStreamingState>,
    /// Receiver for completed responses.
    response_receiver: Option<ChatCompletionReceiver>,
    /// The chat session (created lazily).
    session: Option<ChatSession<DynamicToolsAgent>>,
    /// Yanked messages providing context for the chat.
    yanked_messages: Vec<agent_inbox_protocol::Message>,
    /// Dynamic tools from yanked messages, shared with the agent.
    dynamic_tools: SharedDynamicTools,
}

impl std::fmt::Debug for AgentDialogContentData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentDialogContentData")
            .field("input", &self.input)
            .field("cursor_pos", &self.cursor_pos)
            .field("messages_count", &self.messages.len())
            .field("streaming", &self.streaming)
            .finish_non_exhaustive()
    }
}

/// A formatted line with role indicator for rendering.
#[derive(Debug, Clone)]
struct FormattedLine {
    /// The role prefix (e.g., "[User]", "[Assistant]").
    role_prefix: Option<String>,
    /// The text content of the line.
    text: String,
    /// The role for coloring.
    role: MessageRole,
}

impl AgentDialogContentData {
    /// Creates new agent dialog content data.
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
            messages: Vec::new(),
            scroll: ScrollState::new(),
            formatted_lines: Vec::new(),
            last_width: -1,
            streaming: false,
            streaming_state: None,
            response_receiver: None,
            session: None,
            yanked_messages: Vec::new(),
            dynamic_tools: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Yanks a message into the context.
    ///
    /// The message is queued for inclusion as a document block when the user
    /// submits their next message and is shown in the pending section.
    /// Tools from the message are registered for the agent to use.
    pub fn yank_message(
        &mut self,
        mut message: agent_inbox_protocol::Message,
        origins: Option<Vec<ToolOrigin>>,
    ) {
        let origins = origins.unwrap_or_default();
        let mut tools = self.dynamic_tools.write().unwrap();
        let mut used_names: HashSet<String> =
            tools.iter().map(|tool| tool.param.name.clone()).collect();
        let mut display_tools = Vec::with_capacity(message.tools.len());
        for (idx, tool_param) in message.tools.iter().enumerate() {
            let origin = origins.get(idx);
            let display_name = tool_display_name(&tool_param.name, origin);
            let unique_display_name =
                unique_tool_name(&display_name, &message.msg_id, &mut used_names);
            let original_name = tool_param.name.clone();
            let expects_object = schema_expects_object(&tool_param.input_schema);
            let mut display_param = tool_param.clone();
            display_param.name = unique_display_name;
            let base_url = origin.map(|o| o.base_url.clone()).unwrap_or_default();
            let dynamic_tool = DynamicTool::new(
                display_param.clone(),
                original_name,
                expects_object,
                message.msg_id.clone(),
                base_url,
            );
            tools.push(Arc::new(dynamic_tool));
            display_tools.push(display_param);
        }
        message.tools = display_tools;

        self.last_width = -1; // Force reformat

        self.yanked_messages.push(message);
    }

    /// Returns the number of yanked messages.
    pub fn yanked_count(&self) -> usize {
        self.yanked_messages.len()
    }

    /// Clears all yanked messages and their tools.
    pub fn clear_yanked(&mut self) {
        self.yanked_messages.clear();
        let mut tools = self.dynamic_tools.write().unwrap();
        tools.clear();
        self.last_width = -1;
    }

    /// Ensures the chat session is initialized.
    fn ensure_session(&mut self) -> bool {
        if self.session.is_some() {
            return true;
        }

        let client = match Anthropic::new(None) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let config = ChatConfig::new()
            .with_model(Model::Known(KnownModel::ClaudeHaiku45))
            .with_max_tokens(4096)
            .with_caching(false);

        let agent = DynamicToolsAgent::new(config, Arc::clone(&self.dynamic_tools));
        self.session = Some(ChatSession::with_agent(client, agent));
        true
    }

    /// Returns a reference to the input buffer.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Returns the cursor position.
    pub fn cursor_pos(&self) -> usize {
        self.cursor_pos
    }

    /// Returns whether streaming is in progress.
    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    /// Inserts a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        let byte_pos = self.byte_pos_for_cursor();
        self.input.insert(byte_pos, ch);
        self.cursor_pos += 1;
    }

    /// Deletes the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            let byte_pos = self.byte_pos_for_cursor();
            let ch_len = self.input[byte_pos..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            if ch_len > 0 {
                self.input.drain(byte_pos..byte_pos + ch_len);
            }
        }
    }

    /// Deletes the character at the cursor (delete).
    pub fn delete(&mut self) {
        let byte_pos = self.byte_pos_for_cursor();
        if byte_pos < self.input.len() {
            let ch_len = self.input[byte_pos..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            if ch_len > 0 {
                self.input.drain(byte_pos..byte_pos + ch_len);
            }
        }
    }

    /// Moves cursor left.
    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Moves cursor right.
    pub fn cursor_right(&mut self) {
        let char_count = self.input.chars().count();
        if self.cursor_pos < char_count {
            self.cursor_pos += 1;
        }
    }

    /// Moves cursor to start.
    pub fn cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Moves cursor to end.
    pub fn cursor_end(&mut self) {
        self.cursor_pos = self.input.chars().count();
    }

    /// Converts cursor position (in characters) to byte position.
    fn byte_pos_for_cursor(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }

    /// Submits the current input as a user message.
    pub fn submit(&mut self) {
        if self.input.is_empty() || self.streaming {
            return;
        }

        let user_text = std::mem::take(&mut self.input);
        self.cursor_pos = 0;

        let content = self.build_message_param_content(&user_text);
        push_or_merge_message(
            &mut self.messages,
            MessageParam::new(content.clone(), MessageRole::User),
        );
        self.last_width = -1; // Force reformat

        // Initialize session if needed
        if !self.ensure_session() {
            push_or_merge_message(
                &mut self.messages,
                MessageParam::new(
                    MessageParamContent::String(
                        "Error: Failed to initialize chat session. Check ANTHROPIC_API_KEY."
                            .to_string(),
                    ),
                    MessageRole::Assistant,
                ),
            );
            return;
        }

        // Create streaming state
        let streaming_state = Arc::new(Mutex::new(StreamingState::new()));
        self.streaming_state = Some(Arc::clone(&streaming_state));
        self.streaming = true;

        // Add placeholder for assistant response
        push_or_merge_message(
            &mut self.messages,
            MessageParam::new(
                MessageParamContent::String(String::new()),
                MessageRole::Assistant,
            ),
        );

        // Spawn async task to send message
        let (tx, rx) = mpsc::channel::<(ChatSession<DynamicToolsAgent>, Result<()>)>();
        self.response_receiver = Some(rx);

        // Take ownership of session temporarily
        let mut session = self.session.take().unwrap();

        // Build message with yanked context as document blocks
        let message = MessageParam::new(content, MessageRole::User);

        // Clear yanked messages after including them in the message
        self.yanked_messages.clear();

        let handle = tokio::runtime::Handle::current();
        handle.spawn(async move {
            let mut renderer = StreamingRenderer::new(streaming_state);
            let result = session.send_message(message, &mut renderer).await;
            let io_result = result.map_err(|e| std::io::Error::other(e.to_string()));
            let _ = tx.send((session, io_result));
        });
    }

    /// Builds message content that includes yanked messages as document blocks.
    fn build_message_param_content(&self, user_text: &str) -> MessageParamContent {
        if self.yanked_messages.is_empty() {
            return MessageParamContent::String(user_text.to_string());
        }

        let mut content_blocks = Vec::new();

        // Add each yanked message as a document block
        for (i, msg) in self.yanked_messages.iter().enumerate() {
            let title = format!("Message {} (from: {})", i + 1, msg.from.as_str());
            let doc_block = document_block_from_message(msg, Some(title));
            content_blocks.push(ContentBlock::Document(doc_block));
        }

        // Add the user's text as the final content block
        content_blocks.push(ContentBlock::Text(TextBlock::new(user_text)));

        MessageParamContent::Array(content_blocks)
    }

    /// Polls for completed streaming response.
    pub fn poll_response(&mut self) {
        if !self.streaming {
            return;
        }

        // Update display from streaming state
        if let Some(ref streaming_state) = self.streaming_state {
            let guard = streaming_state.lock().unwrap();
            if let Some(last_msg) = self.messages.last_mut() {
                if last_msg.role == MessageRole::Assistant {
                    match last_msg.content {
                        MessageParamContent::String(ref mut content) => {
                            if guard.text != *content {
                                *content = guard.text.clone();
                                self.last_width = -1; // Force reformat
                            }
                        }
                        MessageParamContent::Array(_) => {
                            last_msg.content = MessageParamContent::String(guard.text.clone());
                            self.last_width = -1; // Force reformat
                        }
                    }
                }
            }
        }

        // Check for completion
        if let Some(ref rx) = self.response_receiver {
            match rx.try_recv() {
                Ok((session, result)) => {
                    self.session = Some(session);
                    self.streaming = false;
                    self.streaming_state = None;
                    self.response_receiver = None;

                    if let Err(e) = result {
                        if let Some(last_msg) = self.messages.last_mut() {
                            if last_msg.role == MessageRole::Assistant {
                                let needs_error = matches!(
                                    last_msg.content,
                                    MessageParamContent::String(ref content) if content.is_empty()
                                );
                                if needs_error {
                                    last_msg.content =
                                        MessageParamContent::String(format!("Error: {}", e));
                                }
                            }
                        }
                    }
                    self.last_width = -1;
                }
                Err(TryRecvError::Empty) => {
                    // Still streaming
                }
                Err(TryRecvError::Disconnected) => {
                    self.streaming = false;
                    self.streaming_state = None;
                    self.response_receiver = None;
                    if let Some(last_msg) = self.messages.last_mut() {
                        if last_msg.role == MessageRole::Assistant {
                            let needs_error = matches!(
                                last_msg.content,
                                MessageParamContent::String(ref content) if content.is_empty()
                            );
                            if needs_error {
                                last_msg.content = MessageParamContent::String(
                                    "Error: Stream disconnected".to_string(),
                                );
                            }
                        }
                    }
                    self.last_width = -1;
                }
            }
        }
    }

    /// Ensures formatted lines are up to date.
    fn ensure_formatted(&mut self, width: i16) {
        if width == self.last_width {
            return;
        }
        self.last_width = width;
        self.formatted_lines.clear();

        if width <= 0 {
            return;
        }

        let width = width as usize;
        let role_width = 12; // "[Assistant] " or "[User]      "
        let content_width = width.saturating_sub(role_width);

        if content_width == 0 {
            return;
        }

        for msg in &self.messages {
            let role_str = match msg.role {
                MessageRole::User => "[User]",
                MessageRole::Assistant => "[Assistant]",
            };

            // Wrap content to fit
            let content = format_message_param_content_for_display(&msg.content);
            let lines = wrap_text(&content, content_width);

            for (i, line) in lines.iter().enumerate() {
                if i == 0 {
                    self.formatted_lines.push(FormattedLine {
                        role_prefix: Some(format!("{:<12}", role_str)),
                        text: line.clone(),
                        role: msg.role,
                    });
                } else {
                    self.formatted_lines.push(FormattedLine {
                        role_prefix: Some(" ".repeat(role_width)),
                        text: line.clone(),
                        role: msg.role,
                    });
                }
            }

            // Add empty line after each message
            self.formatted_lines.push(FormattedLine {
                role_prefix: None,
                text: String::new(),
                role: msg.role,
            });
        }

        if !self.yanked_messages.is_empty() {
            let content = format_pending_yanks(&self.yanked_messages);
            let lines = wrap_text(&content, content_width);
            let role_prefix = format!("{:<12}", "[Yanked]");
            for (i, line) in lines.iter().enumerate() {
                if i == 0 {
                    self.formatted_lines.push(FormattedLine {
                        role_prefix: Some(role_prefix.clone()),
                        text: line.clone(),
                        role: MessageRole::User,
                    });
                } else {
                    self.formatted_lines.push(FormattedLine {
                        role_prefix: Some(" ".repeat(role_width)),
                        text: line.clone(),
                        role: MessageRole::User,
                    });
                }
            }

            self.formatted_lines.push(FormattedLine {
                role_prefix: None,
                text: String::new(),
                role: MessageRole::User,
            });
        }
    }

    /// Scrolls up by the given amount.
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll.scroll_up(amount);
    }

    /// Scrolls down by the given amount.
    pub fn scroll_down(&mut self, amount: usize, visible_rows: usize) {
        let max_offset = ScrollState::max_offset(self.formatted_lines.len(), visible_rows);
        self.scroll.scroll_down(amount, max_offset);
    }

    /// Scrolls to the top.
    pub fn scroll_to_top(&mut self) {
        self.scroll.scroll_to_top();
    }

    /// Scrolls to the bottom.
    pub fn scroll_to_bottom(&mut self, visible_rows: usize) {
        self.scroll
            .scroll_to_bottom(visible_rows, self.formatted_lines.len());
    }

    /// Updates the widget state.
    pub fn update(&mut self, tree: &mut WindowTree, win: WindowId) {
        self.poll_response();
        let width = tree.get(win).state.rect.size.cols;
        self.ensure_formatted(width);
        tree.get_mut(win).mark_repaint();
    }

    /// Renders the widget.
    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        let width = tree.get(win).state.rect.size.cols;
        let rows = tree.get(win).state.rect.size.rows;

        if matches!(mode, RenderMode::CursorOnly) {
            if self.streaming {
                return Ok(CursorBehavior::Hidden);
            }
            // Return cursor position in input field
            let (display_start, _display_width) = self.input_display_window(width, self.streaming);
            let input_row = rows.saturating_sub(1);
            let prompt_len = 2; // "> "
            let spinner_offset = if self.streaming { 1 } else { 0 };
            let cursor_offset = self.cursor_pos.saturating_sub(display_start) as i16;
            let cursor_col =
                (prompt_len + spinner_offset + cursor_offset).min(width.saturating_sub(1));
            return Ok(CursorBehavior::Positioned {
                row: input_row,
                col: cursor_col,
                state: CursorState::VeryVisible,
            });
        }

        self.ensure_formatted(width);

        // Calculate layout: messages take all but last row, input is on last row
        let message_rows = rows.saturating_sub(1).max(0) as usize;
        let input_row = rows.saturating_sub(1);

        let mut renderer = UiRenderer::new(ctx, out);
        let width_usize = width.max(0) as usize;

        // Auto-scroll to bottom when streaming
        if self.streaming {
            self.scroll_to_bottom(message_rows);
        }

        // Render message area
        let scroll_offset = self.scroll.offset();
        for row in 0..message_rows {
            renderer.move_cursor(tree.get(win), row as i16, 0)?;

            let line_idx = scroll_offset + row;
            let color = ColorId::Normal;

            if let Some(formatted) = self.formatted_lines.get(line_idx) {
                // Color based on role
                let role_color = match formatted.role {
                    MessageRole::User => ColorId::IndexAuthor,
                    MessageRole::Assistant => ColorId::Normal,
                };

                if let Some(ref prefix) = formatted.role_prefix {
                    renderer.set_color_by_id(role_color)?;
                    renderer.write_str(prefix)?;
                }

                renderer.set_color_by_id(color)?;
                let padded = pad_string(&formatted.text, width_usize.saturating_sub(12));
                renderer.write_str(&padded)?;
            } else {
                renderer.set_color_by_id(color)?;
                renderer.write_str(&" ".repeat(width_usize))?;
            }
        }

        // Render input field
        renderer.move_cursor(tree.get(win), input_row, 0)?;
        renderer.set_color_by_id(ColorId::Normal)?;
        renderer.write_str("> ")?;

        let input_width = width_usize.saturating_sub(2);
        let spinner = if self.streaming {
            spinner_frame_char()
        } else {
            None
        };
        let spinner_width = if spinner.is_some() { 1 } else { 0 };
        let input_display_width = input_width.saturating_sub(spinner_width);
        let display_start = self.input_display_window(width, self.streaming).0;
        let display_start_byte = self.byte_pos_for_char(display_start);
        let input_display = self.input.get(display_start_byte..).unwrap_or("");
        if let Some(ch) = spinner {
            renderer.write_str(&ch.to_string())?;
        }
        let padded_input = pad_string(input_display, input_display_width);
        renderer.write_str(&padded_input)?;

        // Return cursor position in input field
        let prompt_len = 2; // "> "
        let spinner_offset = if self.streaming { 1 } else { 0 };
        let cursor_offset = self.cursor_pos.saturating_sub(display_start) as i16;
        let cursor_col = (prompt_len + spinner_offset + cursor_offset).min(width.saturating_sub(1));

        if self.streaming {
            Ok(CursorBehavior::Hidden)
        } else {
            Ok(CursorBehavior::Positioned {
                row: input_row,
                col: cursor_col,
                state: CursorState::VeryVisible,
            })
        }
    }

    fn input_display_window(&self, width: i16, streaming: bool) -> (usize, usize) {
        let width_usize = width.max(0) as usize;
        let input_width = width_usize.saturating_sub(2);
        let spinner_width = if streaming { 1 } else { 0 };
        let display_width = input_width.saturating_sub(spinner_width);
        let input_chars = self.input.chars().count();
        let display_start = input_chars.saturating_sub(display_width);
        (display_start, display_width)
    }

    fn byte_pos_for_char(&self, char_pos: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }
}

impl Default for AgentDialogContentData {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps text to fit within the specified width.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        let mut current_width = 0;

        for word in paragraph.split_whitespace() {
            let word_width = word.chars().count();

            if current_width == 0 {
                current_line = word.to_string();
                current_width = word_width;
            } else if current_width + 1 + word_width <= width {
                current_line.push(' ');
                current_line.push_str(word);
                current_width += 1 + word_width;
            } else {
                lines.push(current_line);
                current_line = word.to_string();
                current_width = word_width;
            }
        }

        if !current_line.is_empty() || paragraph.is_empty() {
            lines.push(current_line);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn spinner_frame_char() -> Option<char> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    let time_us = now.as_micros();
    if SPINNERS.is_empty() {
        return None;
    }
    let spinner_index = ((time_us / 1_000_000) % SPINNERS.len() as u128) as usize;
    let (hertz, sequence) = SPINNERS[spinner_index];
    if hertz == 0 || sequence.is_empty() {
        return None;
    }
    let ticks = time_us.saturating_mul(hertz as u128) / 1_000_000;
    let frame = (ticks % sequence.len() as u128) as usize;
    Some(sequence[frame])
}

/// Formats an agent-inbox-protocol Message as a document for the LLM context.
fn format_message_as_document(message: &agent_inbox_protocol::Message) -> String {
    let mut doc = String::new();
    doc.push_str(&format!("From: {}\n", message.from.as_str()));
    doc.push_str(&format!("Date: {}\n", message.date));
    doc.push_str(&format!("Message ID: {}\n", message.msg_id.as_str()));
    doc.push_str("\n--- Body ---\n");
    doc.push_str(message.body.as_str());

    if !message.tools.is_empty() {
        doc.push_str("\n\n--- Tools ---\n");
        for tool in &message.tools {
            doc.push_str(&format_tool_for_document(tool));
            doc.push('\n');
        }
    }

    doc
}

/// Builds a document block from a yanked message for display and context.
fn document_block_from_message(
    message: &agent_inbox_protocol::Message,
    title: Option<String>,
) -> DocumentBlock {
    let doc_content = format_message_as_document(message);
    let source = PlainTextSource::new(doc_content);
    let title = title.unwrap_or_else(|| format!("Message (from: {})", message.from.as_str()));
    DocumentBlock::new_with_plain_text(source).with_title(title)
}

/// Formats a message parameter content for display in the chat UI.
fn format_message_param_content_for_display(content: &MessageParamContent) -> String {
    match content {
        MessageParamContent::String(text) => text.clone(),
        MessageParamContent::Array(blocks) => {
            let mut display = String::new();
            for (i, block) in blocks.iter().enumerate() {
                if i > 0 {
                    display.push_str("\n\n");
                }
                display.push_str(&format_content_block_for_display(block));
            }
            display
        }
    }
}

/// Formats a single content block for display.
fn format_content_block_for_display(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => format!("[Text]\n{}", text.text),
        ContentBlock::Document(doc) => format_document_block_for_display(doc),
        ContentBlock::ToolUse(tool) => format!("[ToolUse]\nName: {}", tool.name),
        ContentBlock::ToolResult(result) => {
            let is_error = result.is_error.unwrap_or(false);
            format!(
                "[ToolResult]\nTool Use ID: {}\nError: {}",
                result.tool_use_id, is_error
            )
        }
        ContentBlock::Image(_) => "[Image]".to_string(),
        ContentBlock::ServerToolUse(_) => "[ServerToolUse]".to_string(),
        ContentBlock::WebSearchToolResult(_) => "[WebSearchToolResult]".to_string(),
        ContentBlock::Thinking(_) => "[Thinking]".to_string(),
        ContentBlock::RedactedThinking(_) => "[RedactedThinking]".to_string(),
    }
}

/// Formats a document block for display.
fn format_document_block_for_display(block: &DocumentBlock) -> String {
    let mut display = String::new();
    display.push_str("[Document");
    if let Some(title) = &block.title {
        display.push_str(": ");
        display.push_str(title);
    }
    display.push_str("]\n");

    if let Some(context) = &block.context {
        display.push_str("Context: ");
        display.push_str(context);
        display.push('\n');
    }

    match &block.source {
        DocumentSource::PlainText(source) => display.push_str(&source.data),
        DocumentSource::Base64Pdf(_) => display.push_str("(document source: base64 pdf)"),
        DocumentSource::UrlPdf(_) => display.push_str("(document source: url pdf)"),
        DocumentSource::ContentBlock(_) => display.push_str("(document source: content block)"),
    }

    display
}

/// Formats pending yanked messages for display in the agent dialog.
fn format_pending_yanks(messages: &[agent_inbox_protocol::Message]) -> String {
    let mut display = String::new();
    display.push_str("Pending yanked documents (not yet sent):");
    for (i, msg) in messages.iter().enumerate() {
        display.push_str("\n\n");
        let title = format!("Pending {} (from: {})", i + 1, msg.from.as_str());
        let doc_block = document_block_from_message(msg, Some(title));
        display.push_str(&format_document_block_for_display(&doc_block));
    }
    display
}

/// Formats a single tool for inclusion in the document context.
fn format_tool_for_document(tool: &ToolParam) -> String {
    if let Some(ref desc) = tool.description {
        if desc.is_empty() {
            return format!("- **{}**", tool.name);
        }
        return format!("- **{}**: {}", tool.name, desc);
    }
    format!("- **{}**", tool.name)
}

/// Renderer that accumulates streaming text into shared state.
struct StreamingRenderer {
    state: SharedStreamingState,
}

impl StreamingRenderer {
    fn new(state: SharedStreamingState) -> Self {
        Self { state }
    }
}

impl Renderer for StreamingRenderer {
    fn print_text(&mut self, _context: &dyn StreamContext, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut guard = self.state.lock().unwrap();
        guard.text.push_str(text);
    }

    fn should_interrupt(&self) -> bool {
        let guard = self.state.lock().unwrap();
        guard.cancelled.load(Ordering::Relaxed)
    }

    fn start_agent(&mut self, _context: &dyn StreamContext) {}

    fn finish_agent(&mut self, _context: &dyn StreamContext, _stop_reason: Option<&StopReason>) {
        let mut guard = self.state.lock().unwrap();
        guard.complete = true;
    }

    fn print_thinking(&mut self, _context: &dyn StreamContext, _text: &str) {}

    fn print_error(&mut self, _context: &dyn StreamContext, error: &str) {
        let mut guard = self.state.lock().unwrap();
        guard.error = Some(error.to_string());
    }

    fn print_info(&mut self, _context: &dyn StreamContext, _info: &str) {}

    fn start_tool_use(&mut self, _context: &dyn StreamContext, name: &str, _id: &str) {
        let mut guard = self.state.lock().unwrap();
        if !guard.text.is_empty() && !guard.text.ends_with('\n') {
            guard.text.push('\n');
        }
        guard.text.push_str(&format!("[Tool] {}\n", name));
    }

    fn print_tool_input(&mut self, _context: &dyn StreamContext, _partial_json: &str) {}

    fn finish_tool_use(&mut self, _context: &dyn StreamContext) {}

    fn start_tool_result(
        &mut self,
        _context: &dyn StreamContext,
        _tool_use_id: &str,
        _is_error: bool,
    ) {
    }

    fn print_tool_result_text(&mut self, _context: &dyn StreamContext, _text: &str) {}

    fn finish_tool_result(&mut self, _context: &dyn StreamContext) {}

    fn finish_response(&mut self, _context: &dyn StreamContext) {}

    fn print_interrupted(&mut self, _context: &dyn StreamContext) {}
}

/// Agent dialog window wrapper.
pub struct AgentDialog {
    window: WindowId,
}

impl AgentDialog {
    /// Creates a new agent dialog.
    pub fn new(tree: &mut WindowTree) -> Self {
        let dialog = tree.add_dialog(WindowType::DlgAgent);
        {
            let borrowed = tree.get_mut(dialog);
            borrowed.help_data = Some(HelpData::from_items(vec![
                HelpItem::new("Esc", "Return to previous view"),
                HelpItem::new("Enter", "Send message"),
                HelpItem::new("PgUp/PgDn", "Scroll"),
            ]));
        }

        let content = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        {
            let borrowed = tree.get_mut(content);
            borrowed.set_widget(WindowWidget::AgentDialog(Box::default()));
            borrowed.state.visible = true;
            borrowed.mark_recalc_repaint();
        }

        let sbar = StatusBar::new(tree);
        sbar.set_title(tree, "Agent Chat - Esc to return, Enter to send");

        tree.add_child(dialog, content);
        tree.add_child(dialog, sbar.window_id());

        // Mark dialog for repaint
        tree.get_mut(dialog).mark_recalc_repaint();

        Self { window: dialog }
    }

    /// Returns the underlying dialog window ID.
    pub fn window_id(&self) -> WindowId {
        self.window
    }
}

pub fn agent_dialog_content_window(tree: &WindowTree, win: WindowId) -> Option<WindowId> {
    if tree.get(win).window_type != WindowType::DlgAgent {
        return None;
    }

    tree.get(win)
        .children
        .iter()
        .copied()
        .find(|child| tree.get(*child).window_type == WindowType::Custom)
}

/// Scrolls the agent dialog content if the window is an agent dialog.
///
/// Returns true if the scroll was handled.
pub fn agent_dialog_scroll(
    tree: &mut WindowTree,
    win: WindowId,
    action: crate::action::Action,
) -> bool {
    use crate::action::Action;

    if tree.get(win).window_type != WindowType::DlgAgent {
        return false;
    }

    let content = tree
        .get(win)
        .children
        .iter()
        .copied()
        .find(|child| tree.get(*child).window_type == WindowType::Custom);

    let Some(content) = content else {
        return false;
    };

    let visible_rows = tree.get(content).state.rect.size.rows.max(0) as usize;
    let visible_rows = visible_rows.saturating_sub(1); // Account for input line

    let Some(WindowWidget::AgentDialog(data)) = tree.get_mut(content).widget_mut() else {
        return false;
    };

    match action {
        Action::NextEntry | Action::NextLine => {
            data.scroll_down(1, visible_rows);
        }
        Action::PrevEntry | Action::PrevLine => {
            data.scroll_up(1);
        }
        Action::NextPage | Action::HalfDown => {
            let amount = visible_rows.max(1) / 2;
            data.scroll_down(amount, visible_rows);
        }
        Action::PrevPage | Action::HalfUp => {
            let amount = visible_rows.max(1) / 2;
            data.scroll_up(amount);
        }
        Action::FirstEntry | Action::TopPage => {
            data.scroll_to_top();
        }
        Action::LastEntry | Action::BottomPage => {
            data.scroll_to_bottom(visible_rows);
        }
        _ => return false,
    }

    tree.get_mut(content).mark_repaint();
    true
}

/// Polls the agent dialog for streaming updates.
///
/// Returns true if the dialog needs to be repainted.
pub fn agent_dialog_poll(tree: &mut WindowTree, win: WindowId) -> bool {
    if tree.get(win).window_type != WindowType::DlgAgent {
        return false;
    }

    let content = tree
        .get(win)
        .children
        .iter()
        .copied()
        .find(|child| tree.get(*child).window_type == WindowType::Custom);

    let Some(content) = content else {
        return false;
    };

    let Some(WindowWidget::AgentDialog(data)) = tree.get_mut(content).widget_mut() else {
        return false;
    };

    if !data.is_streaming() {
        return false;
    }

    data.poll_response();
    tree.get_mut(content).mark_repaint();
    true
}

/// Handles key input for the agent dialog.
///
/// Returns true if the key was handled.
pub fn agent_dialog_handle_key(
    tree: &mut WindowTree,
    win: WindowId,
    key: crossterm::event::KeyEvent,
) -> bool {
    use crossterm::event::KeyCode;
    use crossterm::event::KeyModifiers;

    if tree.get(win).window_type != WindowType::DlgAgent {
        return false;
    }

    let content = tree
        .get(win)
        .children
        .iter()
        .copied()
        .find(|child| tree.get(*child).window_type == WindowType::Custom);

    let Some(content) = content else {
        return false;
    };

    let Some(WindowWidget::AgentDialog(data)) = tree.get_mut(content).widget_mut() else {
        return false;
    };

    match key.code {
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            data.insert_char(c);
            tree.get_mut(content).mark_repaint();
            true
        }
        KeyCode::Backspace => {
            data.backspace();
            tree.get_mut(content).mark_repaint();
            true
        }
        KeyCode::Delete => {
            data.delete();
            tree.get_mut(content).mark_repaint();
            true
        }
        KeyCode::Left => {
            data.cursor_left();
            tree.get_mut(content).mark_repaint();
            true
        }
        KeyCode::Right => {
            data.cursor_right();
            tree.get_mut(content).mark_repaint();
            true
        }
        KeyCode::Home => {
            data.cursor_home();
            tree.get_mut(content).mark_repaint();
            true
        }
        KeyCode::End => {
            data.cursor_end();
            tree.get_mut(content).mark_repaint();
            true
        }
        KeyCode::Enter => {
            if !data.input().is_empty() && !data.is_streaming() {
                data.submit();
                tree.get_mut(content).mark_repaint();
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_dialog_content_new() {
        let data = AgentDialogContentData::new();
        assert!(data.input.is_empty());
        assert_eq!(data.cursor_pos, 0);
        assert!(data.messages.is_empty());
        assert!(!data.streaming);
    }

    #[test]
    fn insert_char_and_backspace() {
        let mut data = AgentDialogContentData::new();
        data.insert_char('a');
        data.insert_char('b');
        data.insert_char('c');
        assert_eq!(data.input, "abc");
        assert_eq!(data.cursor_pos, 3);

        data.backspace();
        assert_eq!(data.input, "ab");
        assert_eq!(data.cursor_pos, 2);
    }

    #[test]
    fn cursor_movement() {
        let mut data = AgentDialogContentData::new();
        data.insert_char('a');
        data.insert_char('b');
        data.insert_char('c');

        data.cursor_left();
        assert_eq!(data.cursor_pos, 2);

        data.cursor_left();
        assert_eq!(data.cursor_pos, 1);

        data.cursor_right();
        assert_eq!(data.cursor_pos, 2);

        data.cursor_home();
        assert_eq!(data.cursor_pos, 0);

        data.cursor_end();
        assert_eq!(data.cursor_pos, 3);
    }

    #[test]
    fn wrap_text_basic() {
        let lines = wrap_text("hello world", 20);
        assert_eq!(lines, vec!["hello world"]);

        let lines = wrap_text("hello world", 6);
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn wrap_text_empty() {
        let lines = wrap_text("", 20);
        assert_eq!(lines, vec![""]);
    }

    #[test]
    fn wrap_text_newlines() {
        let lines = wrap_text("hello\nworld", 20);
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn agent_dialog_new_builds_tree() {
        let mut tree = WindowTree::new();
        let dialog = AgentDialog::new(&mut tree);
        let win = dialog.window_id();
        assert_eq!(tree.get(win).window_type, WindowType::DlgAgent);
        assert_eq!(tree.get(win).children.len(), 2);
        assert!(tree
            .get(win)
            .children
            .iter()
            .any(|child| tree.get(*child).window_type == WindowType::Custom));
        assert!(tree
            .get(win)
            .children
            .iter()
            .any(|child| tree.get(*child).window_type == WindowType::StatusBar));
    }
}
