#![allow(missing_docs)]

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::sync::Mutex;

use agent_inbox_protocol::Mailbox;
use agent_inbox_protocol::Message;
use agent_inbox_protocol::MessageID;
use agent_inbox_protocol::QueryParameters;
use agent_inbox_protocol::ToolCallResponse;
use serde_json::Map;
use serde_json::Value;

/// Configuration for a single mail server.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// The service name (used as prefix when not merging).
    pub name: String,
    /// The base URL for the agent-inbox-protocol server.
    pub base_url: String,
    /// If true, mailboxes appear at top level and merge with same-named mailboxes from other
    /// sources. If false, mailboxes are prefixed with the service name (e.g., "Work/INBOX").
    pub merge: bool,
}

impl ServerConfig {
    /// Creates a new server configuration.
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, merge: bool) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            merge,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolOrigin {
    pub base_url: String,
    pub label: String,
}

impl ToolOrigin {
    pub fn new(base_url: impl Into<String>, label: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let label = label.into();
        let label = if label.is_empty() {
            base_url.clone()
        } else {
            label
        };
        Self { base_url, label }
    }

    pub fn from_server_config(config: &ServerConfig) -> Self {
        Self::new(config.base_url.clone(), config.name.clone())
    }
}

pub fn tool_display_name(tool_name: &str, origin: Option<&ToolOrigin>) -> String {
    let Some(origin) = origin else {
        return tool_name.to_string();
    };
    let label = origin.label.trim();
    if label.is_empty() {
        return tool_name.to_string();
    }
    let suffix = format!("_{}", label);
    if tool_name.ends_with(&suffix) {
        tool_name.to_string()
    } else {
        format!("{}{}", tool_name, suffix)
    }
}

/// Dirty flags for selective redrawing.
pub struct DirtyFlags {
    help_bar: AtomicBool,
    sidebar: AtomicBool,
    index: AtomicBool,
    index_bar: AtomicBool,
    pager: AtomicBool,
    pager_bar: AtomicBool,
    message: AtomicBool,
}

impl DirtyFlags {
    pub fn new() -> Self {
        Self {
            help_bar: AtomicBool::new(true),
            sidebar: AtomicBool::new(true),
            index: AtomicBool::new(true),
            index_bar: AtomicBool::new(true),
            pager: AtomicBool::new(true),
            pager_bar: AtomicBool::new(true),
            message: AtomicBool::new(true),
        }
    }

    pub fn mark_all(&self) {
        self.help_bar.store(true, Ordering::Relaxed);
        self.sidebar.store(true, Ordering::Relaxed);
        self.index.store(true, Ordering::Relaxed);
        self.index_bar.store(true, Ordering::Relaxed);
        self.pager.store(true, Ordering::Relaxed);
        self.pager_bar.store(true, Ordering::Relaxed);
        self.message.store(true, Ordering::Relaxed);
    }

    pub fn mark_message_views(&self) {
        self.index.store(true, Ordering::Relaxed);
        self.index_bar.store(true, Ordering::Relaxed);
        self.pager.store(true, Ordering::Relaxed);
        self.pager_bar.store(true, Ordering::Relaxed);
        self.message.store(true, Ordering::Relaxed);
    }

    pub fn mark_mailbox_change(&self) {
        self.sidebar.store(true, Ordering::Relaxed);
        self.mark_message_views();
    }

    pub fn is_help_bar_dirty(&self) -> bool {
        self.help_bar.load(Ordering::Relaxed)
    }

    pub fn set_help_bar_dirty(&self, value: bool) {
        self.help_bar.store(value, Ordering::Relaxed);
    }

    pub fn is_sidebar_dirty(&self) -> bool {
        self.sidebar.load(Ordering::Relaxed)
    }

    pub fn set_sidebar_dirty(&self, value: bool) {
        self.sidebar.store(value, Ordering::Relaxed);
    }

    pub fn is_index_dirty(&self) -> bool {
        self.index.load(Ordering::Relaxed)
    }

    pub fn set_index_dirty(&self, value: bool) {
        self.index.store(value, Ordering::Relaxed);
    }

    pub fn is_index_bar_dirty(&self) -> bool {
        self.index_bar.load(Ordering::Relaxed)
    }

    pub fn set_index_bar_dirty(&self, value: bool) {
        self.index_bar.store(value, Ordering::Relaxed);
    }

    pub fn is_pager_dirty(&self) -> bool {
        self.pager.load(Ordering::Relaxed)
    }

    pub fn set_pager_dirty(&self, value: bool) {
        self.pager.store(value, Ordering::Relaxed);
    }

    pub fn is_pager_bar_dirty(&self) -> bool {
        self.pager_bar.load(Ordering::Relaxed)
    }

    pub fn set_pager_bar_dirty(&self, value: bool) {
        self.pager_bar.store(value, Ordering::Relaxed);
    }

    pub fn is_message_dirty(&self) -> bool {
        self.message.load(Ordering::Relaxed)
    }

    pub fn set_message_dirty(&self, value: bool) {
        self.message.store(value, Ordering::Relaxed);
    }
}

/// Number of lines to keep between cursor and edge of viewport (like Vim's scrolloff).
const SCROLLOFF: usize = 3;

/// Internal mailbox state used by the inbox.
#[derive(Clone)]
pub struct MailboxState {
    pub mailbox: Mailbox,
    pub selected_message: usize,
    pub scroll_offset: usize,
    pub tool_origins: HashMap<MessageID, Vec<ToolOrigin>>,
}

impl MailboxState {
    pub fn new(mailbox: Mailbox) -> Self {
        Self::new_with_origins(mailbox, HashMap::new())
    }

    pub fn new_with_origins(
        mailbox: Mailbox,
        tool_origins: HashMap<MessageID, Vec<ToolOrigin>>,
    ) -> Self {
        Self {
            mailbox,
            selected_message: 0,
            scroll_offset: 0,
            tool_origins,
        }
    }

    pub fn tool_origins_for_message(&self, message_id: &MessageID) -> Option<&[ToolOrigin]> {
        self.tool_origins
            .get(message_id)
            .map(|origins| origins.as_slice())
    }
}

#[derive(Clone, Debug)]
pub struct ToolFormField {
    pub name: String,
    pub value: String,
    pub schema_type: String,
    pub description: Option<String>,
}

/// State for auto-complete generation in a tool form.
#[derive(Clone, Debug)]
pub struct AutoCompleteState {
    /// The field values being filled by auto-complete.
    pub pending_values: HashMap<String, String>,
    /// Flag to signal cancellation.
    pub cancelled: Arc<AtomicBool>,
    /// Whether generation is still in progress.
    pub in_progress: bool,
    /// Whether new streamed content is available.
    pub dirty: bool,
    /// Error message if auto-complete failed.
    pub error: Option<String>,
    /// Whether to move focus to submit when done.
    pub move_to_submit: bool,
}

impl AutoCompleteState {
    /// Creates a new auto-complete state.
    pub fn new(move_to_submit: bool) -> Self {
        Self {
            pending_values: HashMap::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            in_progress: true,
            dirty: false,
            error: None,
            move_to_submit,
        }
    }

    /// Cancels the auto-complete generation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Returns whether the auto-complete has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl Default for AutoCompleteState {
    fn default() -> Self {
        Self::new(false)
    }
}

/// Shared auto-complete state for thread-safe access.
// TODO(codex): Remove alias.
pub type SharedAutoCompleteState = Arc<Mutex<AutoCompleteState>>;

#[derive(Clone, Debug)]
pub struct ToolFormState {
    pub message_id: MessageID,
    pub tool: claudius::ToolParam,
    pub origin: Option<ToolOrigin>,
    pub fields: Vec<ToolFormField>,
    pub focused: usize,
    pub auto_complete: Option<SharedAutoCompleteState>,
}

fn extract_tool_properties(schema: &Value) -> Option<Map<String, Value>> {
    match schema {
        Value::Object(map) => {
            if let Some(props) = map.get("properties").and_then(|props| props.as_object()) {
                return Some(props.clone());
            }
            if let Some(inner) = map.get("schema").or_else(|| map.get("input_schema")) {
                return extract_tool_properties(inner);
            }
            None
        }
        // TODO(claude):  Remove this variant.  Make match an if-let.
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|parsed| extract_tool_properties(&parsed)),
        _ => None,
    }
}

impl ToolFormState {
    pub fn new(
        message_id: MessageID,
        tool: claudius::ToolParam,
        origin: Option<ToolOrigin>,
    ) -> Self {
        let mut fields = Vec::new();
        if let Some(properties) = extract_tool_properties(&tool.input_schema) {
            let mut keys: Vec<String> = properties.keys().cloned().collect();
            keys.sort();
            for key in keys {
                let schema = properties.get(&key).and_then(|v| v.as_object());
                let schema_type = schema
                    .and_then(|s| s.get("type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("string")
                    .to_string();
                let description = schema
                    .and_then(|s| s.get("description"))
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());
                fields.push(ToolFormField {
                    name: key,
                    value: String::new(),
                    schema_type,
                    description,
                });
            }
        }
        Self {
            message_id,
            tool,
            origin,
            fields,
            focused: 0,
            auto_complete: None,
        }
    }

    /// Returns whether auto-complete is currently in progress.
    pub fn is_auto_completing(&self) -> bool {
        self.auto_complete
            .as_ref()
            .map(|ac| {
                let guard = ac.lock().unwrap();
                guard.in_progress && !guard.is_cancelled()
            })
            .unwrap_or(false)
    }

    /// Cancels any in-progress auto-complete.
    pub fn cancel_auto_complete(&mut self) {
        if let Some(ref ac) = self.auto_complete {
            let guard = ac.lock().unwrap();
            guard.cancel();
        }
        self.auto_complete = None;
    }

    /// Applies pending auto-complete values to the fields.
    pub fn apply_auto_complete(&mut self) {
        if let Some(ref ac) = self.auto_complete {
            let pending_values = {
                let guard = ac.lock().unwrap();
                guard.pending_values.clone()
            };
            self.apply_pending_values(&pending_values);
        }
        self.auto_complete = None;
    }

    pub fn apply_pending_values(&mut self, pending_values: &HashMap<String, String>) -> bool {
        let mut changed = false;
        for field in &mut self.fields {
            if let Some(value) = pending_values.get(&field.name) {
                if field.value != *value {
                    field.value = value.clone();
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn focus_len(&self) -> usize {
        self.fields.len() + 2
    }

    pub fn focus_next(&mut self) {
        let len = self.focus_len();
        if len > 0 {
            self.focused = (self.focused + 1) % len;
        }
    }

    pub fn focus_prev(&mut self) {
        let len = self.focus_len();
        if len > 0 {
            self.focused = (self.focused + len - 1) % len;
        }
    }

    pub fn focused_field_mut(&mut self) -> Option<&mut ToolFormField> {
        if self.focused < self.fields.len() {
            Some(&mut self.fields[self.focused])
        } else {
            None
        }
    }

    pub fn is_submit_focused(&self) -> bool {
        self.focused == self.fields.len()
    }

    pub fn is_cancel_focused(&self) -> bool {
        self.focused == self.fields.len() + 1
    }

    pub fn build_input(&self) -> std::result::Result<Value, String> {
        let mut map = Map::new();
        for field in &self.fields {
            let trimmed = field.value.trim();
            let value = if trimmed.is_empty() {
                Value::Null
            } else {
                match field.schema_type.as_str() {
                    "boolean" => {
                        let lowered = trimmed.to_ascii_lowercase();
                        match lowered.as_str() {
                            "true" | "yes" | "y" | "1" => Value::Bool(true),
                            "false" | "no" | "n" | "0" => Value::Bool(false),
                            _ => {
                                return Err(format!(
                                    "Field '{}' expects boolean (true/false/yes/no)",
                                    field.name
                                ));
                            }
                        }
                    }
                    "integer" => trimmed
                        .parse::<i64>()
                        .map(Value::from)
                        .map_err(|_| format!("Field '{}' expects integer", field.name))?,
                    "number" => trimmed
                        .parse::<f64>()
                        .map(Value::from)
                        .map_err(|_| format!("Field '{}' expects number", field.name))?,
                    "object" | "array" => serde_json::from_str(trimmed)
                        .map_err(|_| format!("Field '{}' expects valid JSON", field.name))?,
                    _ => Value::String(field.value.clone()),
                }
            };
            map.insert(field.name.clone(), value);
        }
        Ok(Value::Object(map))
    }

    pub fn render_data(&self) -> ToolFormRenderData {
        ToolFormRenderData {
            tool_name: tool_display_name(&self.tool.name, self.origin.as_ref()),
            tool_description: self.tool.description.clone(),
            fields: self
                .fields
                .iter()
                .map(|field| ToolFormFieldRender {
                    name: field.name.clone(),
                    value: field.value.clone(),
                    description: field.description.clone(),
                })
                .collect(),
            focused: self.focused,
        }
    }
}

#[derive(Clone)]
pub struct ToolFormFieldRender {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
}

#[derive(Clone)]
pub struct ToolFormRenderData {
    pub tool_name: String,
    pub tool_description: Option<String>,
    pub fields: Vec<ToolFormFieldRender>,
    pub focused: usize,
}

/// Render data extracted from InboxState for drawing.
#[derive(Clone)]
pub struct RenderData {
    pub selected_mailbox: usize,
    pub selected_message: usize,
    pub scroll_offset: usize,
    pub mailbox_names: Vec<String>,
    pub messages: Vec<Message>,
    pub selected_message_origins: Option<Vec<ToolOrigin>>,
    pub status_message: String,
    pub search_pattern: Option<String>,
    pub tool_form: Option<ToolFormRenderData>,
    /// Name of tool pending confirmation, if any.
    pub tool_confirm_name: Option<String>,
}

#[derive(Clone)]
pub struct ToolConflictChoice {
    pub tool: claudius::ToolParam,
    pub origin: ToolOrigin,
}

#[derive(Clone)]
pub struct ToolConflictState {
    pub message_id: MessageID,
    pub tool_name: String,
    pub choices: Vec<ToolConflictChoice>,
    pub prefill: Option<Value>,
}

/// Pending tool call awaiting user confirmation.
#[derive(Clone)]
pub struct ToolCallConfirmation {
    /// The tool call request to execute.
    pub request: agent_inbox_protocol::ToolCallRequest,
    /// The name of the tool being invoked.
    pub tool_name: String,
    /// The base URL of the server to call.
    pub base_url: String,
}

/// Inbox state.
pub struct InboxState {
    pub selected_mailbox: usize,
    pub mailbox_states: Vec<MailboxState>,
    pub status_message: String,
    pub dirty: DirtyFlags,
    pub tool_form: Option<ToolFormState>,
    pub tool_conflict: Option<ToolConflictState>,
    pub tool_call_pending: Option<Receiver<std::result::Result<ToolCallResponse, String>>>,
    /// Pending tool call confirmation awaiting user approval.
    pub tool_confirm_pending: Option<ToolCallConfirmation>,
    pub tool_prefix_active: bool,
    pub refresh_requested: bool,
    /// Server-side query parameters to send to all backends on refresh.
    pub server_query: QueryParameters,
}

impl InboxState {
    /// Extracts render data for drawing.
    ///
    /// Returns `None` if there are no mailboxes.
    pub fn render_data(&self) -> Option<RenderData> {
        let mailbox_state = self.mailbox_states.get(self.selected_mailbox)?;
        let selected_message_origins = mailbox_state
            .mailbox
            .messages
            .get(mailbox_state.selected_message)
            .and_then(|message| mailbox_state.tool_origins_for_message(&message.msg_id))
            .map(|origins| origins.to_vec());

        Some(RenderData {
            selected_mailbox: self.selected_mailbox,
            selected_message: mailbox_state.selected_message,
            scroll_offset: mailbox_state.scroll_offset,
            mailbox_names: self
                .mailbox_states
                .iter()
                .map(|m| m.mailbox.name.as_str().to_string())
                .collect(),
            messages: mailbox_state.mailbox.messages.clone(),
            selected_message_origins,
            status_message: self.status_message.clone(),
            search_pattern: self.server_query.search.clone(),
            tool_form: self.tool_form.as_ref().map(ToolFormState::render_data),
            tool_confirm_name: self
                .tool_confirm_pending
                .as_ref()
                .map(|c| c.tool_name.clone()),
        })
    }

    /// Returns a reference to the currently selected mailbox.
    ///
    /// Returns `None` if there are no mailboxes.
    pub fn current_mailbox(&self) -> Option<&MailboxState> {
        self.mailbox_states.get(self.selected_mailbox)
    }

    /// Returns a mutable reference to the currently selected mailbox.
    ///
    /// Returns `None` if there are no mailboxes.
    pub fn current_mailbox_mut(&mut self) -> Option<&mut MailboxState> {
        self.mailbox_states.get_mut(self.selected_mailbox)
    }

    /// Returns a reference to the currently selected message.
    ///
    /// Returns `None` if there are no mailboxes or no messages.
    pub fn selected_message(&self) -> Option<&Message> {
        let mailbox = self.current_mailbox()?;
        mailbox.mailbox.messages.get(mailbox.selected_message)
    }

    pub fn new(mailboxes: Vec<Mailbox>) -> Self {
        let mailbox_states: Vec<MailboxState> =
            mailboxes.into_iter().map(MailboxState::new).collect();
        Self {
            selected_mailbox: 0,
            mailbox_states,
            status_message: "Welcome! Press 'q' to quit, j/k to navigate.".to_string(),
            dirty: DirtyFlags::new(),
            tool_form: None,
            tool_conflict: None,
            tool_call_pending: None,
            tool_confirm_pending: None,
            tool_prefix_active: false,
            refresh_requested: false,
            server_query: QueryParameters::default(),
        }
    }

    pub fn new_with_states(mailbox_states: Vec<MailboxState>) -> Self {
        Self {
            selected_mailbox: 0,
            mailbox_states,
            status_message: "Welcome! Press 'q' to quit, j/k to navigate.".to_string(),
            dirty: DirtyFlags::new(),
            tool_form: None,
            tool_conflict: None,
            tool_call_pending: None,
            tool_confirm_pending: None,
            tool_prefix_active: false,
            refresh_requested: false,
            server_query: QueryParameters::default(),
        }
    }

    /// Selects the next message in the current mailbox.
    ///
    /// Does nothing if there are no mailboxes or the current message is the last one.
    pub fn select_next_message(&mut self) {
        let Some(mailbox) = self.current_mailbox_mut() else {
            return;
        };
        if !mailbox.mailbox.messages.is_empty()
            && mailbox.selected_message < mailbox.mailbox.messages.len() - 1
        {
            mailbox.selected_message += 1;
            let selected = mailbox.selected_message;
            self.status_message = format!("Selected message {}", selected + 1);
            self.dirty.mark_message_views();
        }
    }

    /// Selects the previous message in the current mailbox.
    ///
    /// Does nothing if there are no mailboxes or the current message is the first one.
    pub fn select_prev_message(&mut self) {
        let Some(mailbox) = self.current_mailbox_mut() else {
            return;
        };
        if mailbox.selected_message > 0 {
            mailbox.selected_message -= 1;
            let selected = mailbox.selected_message;
            self.status_message = format!("Selected message {}", selected + 1);
            self.dirty.mark_message_views();
        }
    }

    /// Updates the scroll offset for the current mailbox based on viewport height.
    ///
    /// Implements scrolloff behavior: maintains SCROLLOFF lines between the cursor and
    /// the bottom of the viewport, except when the cursor is in the last SCROLLOFF
    /// messages of the list (where it's allowed to approach the bottom edge).
    ///
    /// Does nothing if there are no mailboxes.
    pub fn update_scroll_offset(&mut self, viewport_height: usize) {
        let Some(mailbox) = self.current_mailbox_mut() else {
            return;
        };
        let selected = mailbox.selected_message;
        let total = mailbox.mailbox.messages.len();
        let scroll = &mut mailbox.scroll_offset;

        if total == 0 || viewport_height == 0 {
            *scroll = 0;
            return;
        }

        // Clamp scrolloff to avoid conflicts in small viewports (centers the cursor)
        let scrolloff = SCROLLOFF.min(viewport_height.saturating_sub(1) / 2);

        // Maximum scroll position (can't scroll past the end).
        let max_scroll = total.saturating_sub(viewport_height);

        // If cursor is above the viewport, scroll up to show it.
        if selected < *scroll {
            *scroll = selected;
        }

        // If cursor is at or past the bottom edge of viewport, scroll down to show it.
        let viewport_bottom = *scroll + viewport_height;
        if selected >= viewport_bottom {
            *scroll = selected.saturating_sub(viewport_height - 1);
        }

        let mut cursor_pos_in_viewport = selected.saturating_sub(*scroll);

        // Top scrolloff
        if cursor_pos_in_viewport < scrolloff {
            *scroll = selected.saturating_sub(scrolloff);
            cursor_pos_in_viewport = selected.saturating_sub(*scroll);
        }

        // Bottom scrolloff
        let ideal_max_pos = viewport_height.saturating_sub(1 + scrolloff);

        if cursor_pos_in_viewport > ideal_max_pos {
            let adjustment = cursor_pos_in_viewport - ideal_max_pos;
            *scroll = (*scroll + adjustment).min(max_scroll);
        }

        // Clamp scroll to valid range.
        if *scroll > max_scroll {
            *scroll = max_scroll;
        }
    }

    /// Selects the next mailbox.
    ///
    /// Does nothing if there are no mailboxes or the current mailbox is the last one.
    pub fn select_next_mailbox(&mut self) {
        if self.mailbox_states.len() > 1 && self.selected_mailbox < self.mailbox_states.len() - 1 {
            self.selected_mailbox += 1;
            if let Some(mailbox) = self.current_mailbox() {
                self.status_message =
                    format!("Selected mailbox: {}", mailbox.mailbox.name.as_str());
            }
            self.dirty.mark_mailbox_change();
        }
    }

    /// Selects the previous mailbox.
    ///
    /// Does nothing if there are no mailboxes or the current mailbox is the first one.
    pub fn select_prev_mailbox(&mut self) {
        if self.selected_mailbox > 0 {
            self.selected_mailbox -= 1;
            if let Some(mailbox) = self.current_mailbox() {
                self.status_message =
                    format!("Selected mailbox: {}", mailbox.mailbox.name.as_str());
            }
            self.dirty.mark_mailbox_change();
        }
    }

    /// Updates the mailboxes with new data from a background refresh.
    ///
    /// Preserves the selected mailbox and message indices where possible.
    pub fn update_mailboxes(&mut self, mailbox_states: Vec<MailboxState>, mark_dirty: bool) {
        // Store current selection state.
        let current_mailbox_name = self
            .mailbox_states
            .get(self.selected_mailbox)
            .map(|m| m.mailbox.name.as_str().to_string());
        let current_message_idx = self
            .mailbox_states
            .get(self.selected_mailbox)
            .map(|m| m.selected_message);

        // Update mailbox states.
        self.mailbox_states = mailbox_states;

        // Restore selection if possible.
        if let Some(name) = current_mailbox_name {
            if let Some(idx) = self
                .mailbox_states
                .iter()
                .position(|m| m.mailbox.name.as_str() == name)
            {
                self.selected_mailbox = idx;
                if let Some(msg_idx) = current_message_idx {
                    let max_idx = self.mailbox_states[idx]
                        .mailbox
                        .messages
                        .len()
                        .saturating_sub(1);
                    self.mailbox_states[idx].selected_message = msg_idx.min(max_idx);
                }
            } else {
                self.selected_mailbox = 0;
            }
        } else {
            self.selected_mailbox = 0;
        }

        self.tool_form = None;
        if mark_dirty {
            self.dirty.mark_all();
        }
    }
}

impl std::fmt::Debug for InboxState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InboxState")
            .field("selected_mailbox", &self.selected_mailbox)
            .field("mailbox_count", &self.mailbox_states.len())
            .field("status_message", &self.status_message)
            .finish()
    }
}
