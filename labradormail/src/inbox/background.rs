use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;
use std::sync::Mutex;

use agent_inbox_protocol::Client;
use agent_inbox_protocol::Mailbox;
use agent_inbox_protocol::MailboxName;
use agent_inbox_protocol::MailboxProvider;
use agent_inbox_protocol::Message;
use agent_inbox_protocol::MessageID;
use agent_inbox_protocol::QueryParameters;
use agent_inbox_protocol::ToolCallRequest;
use agent_inbox_protocol::ToolCallResponse;
use async_trait::async_trait;
use claudius::ThinkingConfig;

use super::fetch_mailboxes;
use super::state::AutoCompleteState;
use super::state::InboxState;
use super::state::MailboxState;
use super::state::ServerConfig;
use super::state::SharedAutoCompleteState;
use super::state::ToolOrigin;
use super::ui::is_primary_dialog_active;
use super::ui::Windows;

async fn tool_call(
    base_url: &str,
    request: ToolCallRequest,
) -> std::result::Result<ToolCallResponse, String> {
    let mut client = Client::new(base_url);
    client
        .tool_call(request)
        .await
        .map_err(|e| format!("HTTP tool call failed: {}", e))
}

pub fn spawn_tool_call(
    base_url: String,
    request: ToolCallRequest,
) -> Receiver<std::result::Result<ToolCallResponse, String>> {
    let (tx, rx) = mpsc::channel();
    let handle = tokio::runtime::Handle::current();
    handle.spawn(async move {
        let result = tool_call(&base_url, request).await;
        let _ = tx.send(result);
    });
    rx
}

/// Spawns a background task to auto-fill tool form fields from the focused field onward.
pub fn spawn_auto_complete(
    state: &mut InboxState,
    limit_to_one: bool,
    move_to_submit: bool,
) -> Option<SharedAutoCompleteState> {
    let form = state.tool_form.as_ref()?;
    let focused = form.focused;
    if focused >= form.fields.len() {
        return None;
    }

    // Get current message context
    let message = match state.selected_message() {
        Some(message) => message.clone(),
        None => {
            return None;
        }
    };
    let email_context = format!(
        "From: {}\nDate: {}\n\n{}",
        message.from.as_str(),
        message.date.format("%Y-%m-%d %H:%M:%S"),
        message.body.as_str()
    );

    // Get tool info
    let tool_name = form.tool.name.clone();
    let tool_description = form.tool.description.clone().unwrap_or_default();
    // Include current values so the model can see form answers thus far
    let end = if limit_to_one {
        (focused + 1).min(form.fields.len())
    } else {
        form.fields.len()
    };
    let prior_fields: Vec<(String, String)> = form
        .fields
        .iter()
        .take(focused)
        .map(|f| (f.name.clone(), f.value.clone()))
        .collect();
    let fields: Vec<FieldSpec> = form
        .fields
        .iter()
        .skip(focused)
        .take(end.saturating_sub(focused))
        .map(|f| FieldSpec {
            name: f.name.clone(),
            schema_type: f.schema_type.clone(),
            description: f.description.clone(),
            current_value: f.value.clone(),
            force_regen: limit_to_one,
        })
        .collect();

    // Create auto-complete state
    let ac_state = Arc::new(Mutex::new(AutoCompleteState::new(move_to_submit)));
    let ac_state_clone = Arc::clone(&ac_state);
    let handle = tokio::runtime::Handle::current();
    handle.spawn(async move {
        run_auto_complete(
            ac_state_clone,
            email_context,
            tool_name,
            tool_description,
            prior_fields,
            fields,
        )
        .await;
    });

    Some(ac_state)
}

#[derive(Clone, Debug)]
struct FieldSpec {
    name: String,
    schema_type: String,
    description: Option<String>,
    current_value: String,
    force_regen: bool,
}

struct AutoFillAgent;

#[async_trait]
impl claudius::Agent for AutoFillAgent {
    async fn max_tokens(&self) -> u32 {
        2048
    }

    async fn thinking(&self) -> Option<ThinkingConfig> {
        Some(ThinkingConfig::enabled(1024))
    }

    async fn model(&self) -> claudius::Model {
        claudius::Model::Known(claudius::KnownModel::ClaudeHaiku45)
    }

    fn stream_label(&self) -> String {
        "AutoFill".to_string()
    }
}

struct FieldStreamRenderer {
    state: SharedAutoCompleteState,
    field_name: String,
    buffer: String,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl FieldStreamRenderer {
    fn new(
        state: SharedAutoCompleteState,
        field_name: String,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            state,
            field_name,
            buffer: String::new(),
            cancelled,
        }
    }

    fn finish(self) -> String {
        self.buffer
    }
}

impl claudius::Renderer for FieldStreamRenderer {
    fn print_text(&mut self, _context: &dyn claudius::StreamContext, text: &str) {
        if text.is_empty() {
            return;
        }
        self.buffer.push_str(text);
        let mut guard = self.state.lock().unwrap();
        guard
            .pending_values
            .insert(self.field_name.clone(), self.buffer.clone());
        guard.dirty = true;
    }

    fn should_interrupt(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn start_agent(&mut self, _context: &dyn claudius::StreamContext) {}

    fn finish_agent(
        &mut self,
        _context: &dyn claudius::StreamContext,
        stop_reason: Option<&claudius::StopReason>,
    ) {
        if let Some(reason) = stop_reason {
            let _ = reason;
        }
    }

    fn print_thinking(&mut self, _context: &dyn claudius::StreamContext, _text: &str) {}

    fn print_error(&mut self, _context: &dyn claudius::StreamContext, error: &str) {
        let _ = error;
    }

    fn print_info(&mut self, _context: &dyn claudius::StreamContext, info: &str) {
        let _ = info;
    }

    fn start_tool_use(&mut self, _context: &dyn claudius::StreamContext, _name: &str, _id: &str) {}

    fn print_tool_input(&mut self, _context: &dyn claudius::StreamContext, _partial_json: &str) {}

    fn finish_tool_use(&mut self, _context: &dyn claudius::StreamContext) {}

    fn start_tool_result(
        &mut self,
        _context: &dyn claudius::StreamContext,
        _tool_use_id: &str,
        _is_error: bool,
    ) {
    }

    fn print_tool_result_text(&mut self, _context: &dyn claudius::StreamContext, _text: &str) {}

    fn finish_tool_result(&mut self, _context: &dyn claudius::StreamContext) {}

    fn finish_response(&mut self, _context: &dyn claudius::StreamContext) {}

    fn print_interrupted(&mut self, _context: &dyn claudius::StreamContext) {}
}

fn is_auto_complete_cancelled(state: &SharedAutoCompleteState) -> bool {
    let guard = state.lock().unwrap();
    guard.is_cancelled()
}

fn build_field_prompt(
    tool_name: &str,
    tool_description: &str,
    prior_fields: &[(String, String)],
    field: &FieldSpec,
    retry_reason: Option<&str>,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("You are filling out a tool form for an email application.\n\n");
    prompt.push_str(&format!(
        "Tool: {}{}\n\n",
        tool_name,
        if tool_description.is_empty() {
            String::new()
        } else {
            format!(" - {}", tool_description)
        }
    ));
    if prior_fields.iter().any(|(_, value)| !value.is_empty()) {
        prompt.push_str("Fields already filled (do not change):\n");
        for (name, value) in prior_fields {
            if value.is_empty() {
                continue;
            }
            prompt.push_str(&format!("- {}: {}\n", name, value));
        }
        prompt.push('\n');
    }
    prompt.push_str("Fill the next field based on the email document.\n\n");
    prompt.push_str(&format!("Field: {}\n", field.name));
    prompt.push_str(&format!("Type: {}\n", field.schema_type));
    if let Some(desc) = &field.description {
        prompt.push_str(&format!("Description: {}\n", desc));
    }
    if !field.current_value.is_empty() {
        prompt.push_str(&format!("Current value: {}\n", field.current_value));
    }
    if field.force_regen {
        prompt.push_str("Generate a new value, even if a current value exists.\n");
    }
    if let Some(reason) = retry_reason {
        prompt.push_str("\nYour previous response was invalid.\n");
        prompt.push_str(reason);
        prompt.push('\n');
    }
    prompt.push_str(&format!(
        "\nRespond with ONLY the value of this field.\n{}\nDo not add labels, JSON objects, or extra text.",
        schema_hint(&field.schema_type)
    ));
    prompt
}

fn schema_hint(schema_type: &str) -> &'static str {
    match schema_type {
        "boolean" => "Return true or false only.",
        "integer" => "Return a whole number, digits only.",
        "number" => "Return a number (integer or decimal) with no extra text.",
        "object" => "Return a JSON object (e.g. {\"key\":\"value\"}).",
        "array" => "Return a JSON array (e.g. [\"item\"]).",
        _ => "Return plain text only, no quotes or extra formatting.",
    }
}

fn normalize_field_value(raw: &str, schema_type: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    match schema_type {
        "boolean" => {
            let lowered = trimmed.to_ascii_lowercase();
            match lowered.as_str() {
                "true" | "yes" | "y" | "1" => Ok("true".to_string()),
                "false" | "no" | "n" | "0" => Ok("false".to_string()),
                _ => Err("Expected boolean: true or false.".to_string()),
            }
        }
        "integer" => trimmed
            .parse::<i64>()
            .map(|val| val.to_string())
            .map_err(|_| "Expected an integer value.".to_string()),
        "number" => trimmed
            .parse::<f64>()
            .map(|val| {
                if val.fract() == 0.0 {
                    format!("{}", val as i64)
                } else {
                    val.to_string()
                }
            })
            .map_err(|_| "Expected a number.".to_string()),
        "object" => {
            let value = serde_json::from_str::<serde_json::Value>(trimmed)
                .map_err(|_| "Expected a JSON object.".to_string())?;
            if !value.is_object() {
                return Err("Expected a JSON object.".to_string());
            }
            serde_json::to_string(&value).map_err(|_| "Invalid JSON object.".to_string())
        }
        "array" => {
            let value = serde_json::from_str::<serde_json::Value>(trimmed)
                .map_err(|_| "Expected a JSON array.".to_string())?;
            if !value.is_array() {
                return Err("Expected a JSON array.".to_string());
            }
            serde_json::to_string(&value).map_err(|_| "Invalid JSON array.".to_string())
        }
        _ => {
            if let Ok(serde_json::Value::String(value)) =
                serde_json::from_str::<serde_json::Value>(trimmed)
            {
                Ok(value)
            } else {
                Ok(trimmed.to_string())
            }
        }
    }
}

/// Runs the auto-fill request asynchronously using DocumentBlock for email context.
async fn run_auto_complete(
    state: SharedAutoCompleteState,
    email_context: String,
    tool_name: String,
    tool_description: String,
    prior_fields: Vec<(String, String)>,
    fields: Vec<FieldSpec>,
) {
    use claudius::Agent;
    use claudius::Anthropic;
    use claudius::Budget;
    use claudius::ContentBlock;
    use claudius::DocumentBlock;
    use claudius::MessageParam;
    use claudius::MessageParamContent;
    use claudius::MessageRole;
    use claudius::PlainTextSource;
    use claudius::TextBlock;

    // Check if cancelled before starting
    {
        let guard = state.lock().unwrap();
        if guard.is_cancelled() {
            return;
        }
    }

    let client = match Anthropic::new(None) {
        Ok(c) => c,
        Err(_) => {
            let mut guard = state.lock().unwrap();
            guard.in_progress = false;
            guard.error = Some("Auto-fill failed to start".to_string());
            guard.dirty = true;
            return;
        }
    };

    let budget = Arc::new(Budget::new_with_rates(10_000_000, 500, 500, 125, 10));
    let mut agent = AutoFillAgent;
    let mut prior_fields = prior_fields;

    for field in &fields {
        if is_auto_complete_cancelled(&state) {
            break;
        }
        let mut attempts = 0;
        let mut last_error: Option<String> = None;
        loop {
            attempts += 1;
            if is_auto_complete_cancelled(&state) {
                break;
            }
            {
                let mut guard = state.lock().unwrap();
                guard
                    .pending_values
                    .insert(field.name.clone(), String::new());
                guard.dirty = true;
            }

            let prompt_text = build_field_prompt(
                &tool_name,
                &tool_description,
                &prior_fields,
                field,
                last_error.as_deref(),
            );

            let source = PlainTextSource::new(email_context.clone());
            let doc_block =
                DocumentBlock::new_with_plain_text(source).with_title("Current Email".to_string());
            let content_blocks = vec![
                ContentBlock::Document(doc_block),
                ContentBlock::Text(TextBlock::new(prompt_text)),
            ];
            let message = MessageParam::new(
                MessageParamContent::Array(content_blocks),
                MessageRole::User,
            );
            let mut messages = vec![message];

            let cancelled = {
                let guard = state.lock().unwrap();
                Arc::clone(&guard.cancelled)
            };
            let mut renderer =
                FieldStreamRenderer::new(Arc::clone(&state), field.name.clone(), cancelled);

            let outcome = agent
                .take_turn_streaming_root(&client, &mut messages, &budget, &mut renderer)
                .await;
            match outcome {
                Ok(outcome) => {
                    let _ = outcome;
                    let raw = renderer.finish();
                    match normalize_field_value(&raw, &field.schema_type) {
                        Ok(normalized) => {
                            let mut guard = state.lock().unwrap();
                            guard.pending_values.insert(field.name.clone(), normalized);
                            guard.dirty = true;
                            let normalized = guard
                                .pending_values
                                .get(&field.name)
                                .cloned()
                                .unwrap_or_default();
                            if !normalized.is_empty() {
                                prior_fields.push((field.name.clone(), normalized));
                            }
                            break;
                        }
                        Err(err) => {
                            last_error = Some(err);
                            if attempts >= 2 {
                                break;
                            }
                            continue;
                        }
                    }
                }
                Err(err) => {
                    let _ = err;
                    let mut guard = state.lock().unwrap();
                    guard.in_progress = false;
                    guard.error = Some("Auto-fill failed while streaming".to_string());
                    guard.dirty = true;
                    return;
                }
            }
        }
    }

    let mut guard = state.lock().unwrap();
    guard.in_progress = false;
    guard.dirty = true;
}

/// Polls and handles pending tool call responses.
pub fn poll_pending_tool_call(windows: &mut Windows) {
    let pending_tool_call = {
        let Some(state) = windows.state_mut() else {
            return;
        };
        state.tool_call_pending.take()
    };
    if let Some(rx) = pending_tool_call {
        match rx.try_recv() {
            Ok(Ok(response)) => {
                let msg = response
                    .message
                    .unwrap_or_else(|| "Tool call succeeded".to_string());
                if let Some(state) = windows.state_mut() {
                    state.status_message = msg;
                    state.refresh_requested = true;
                    state.dirty.set_message_dirty(true);
                }
            }
            Ok(Err(err)) => {
                if let Some(state) = windows.state_mut() {
                    state.status_message = format!("Tool call failed: {}", err);
                    state.dirty.set_message_dirty(true);
                }
            }
            Err(TryRecvError::Empty) => {
                if let Some(state) = windows.state_mut() {
                    state.tool_call_pending = Some(rx);
                }
            }
            Err(TryRecvError::Disconnected) => {
                if let Some(state) = windows.state_mut() {
                    state.status_message = "Tool call failed: worker disconnected".to_string();
                    state.dirty.set_message_dirty(true);
                }
            }
        }
    }
}

/// Polls and handles pending auto-complete responses.
pub fn poll_pending_auto_complete(windows: &mut Windows) {
    let Some(state) = windows.state_mut() else {
        return;
    };
    let Some(ref mut form) = state.tool_form else {
        return;
    };

    let Some(ac_state) = form.auto_complete.clone() else {
        return;
    };

    let (in_progress, dirty, pending_values, error, move_to_submit) = {
        let guard = ac_state.lock().unwrap();
        (
            guard.in_progress,
            guard.dirty,
            guard.pending_values.clone(),
            guard.error.clone(),
            guard.move_to_submit,
        )
    };

    if dirty {
        if form.apply_pending_values(&pending_values) {
            state.dirty.set_pager_dirty(true);
            state.dirty.set_message_dirty(true);
        }
        {
            let mut guard = ac_state.lock().unwrap();
            guard.dirty = false;
        }
    }

    if !in_progress {
        form.auto_complete = None;
        if move_to_submit {
            form.focused = form.fields.len();
        }
        if let Some(err) = error {
            state.status_message = format!("Auto-fill failed: {}", err);
        } else {
            state.status_message = "Auto-fill complete".to_string();
        }
        state.dirty.set_pager_dirty(true);
        state.dirty.set_message_dirty(true);
    }
}

/// Spawn a background task to fetch mailboxes from multiple servers.
pub fn spawn_background_fetch(
    configs: Vec<ServerConfig>,
    query: QueryParameters,
) -> Receiver<std::result::Result<Vec<MailboxState>, String>> {
    let (tx, rx) = mpsc::channel();
    let handle = tokio::runtime::Handle::current();
    handle.spawn(async move {
        let result = fetch_and_merge_mailboxes(&configs, &query).await;
        let _ = tx.send(result);
    });
    rx
}

/// Fetch mailboxes from multiple servers and merge/prefix based on config.
///
/// For servers with `merge=true`, mailboxes retain their original names and are merged
/// with same-named mailboxes from other merge sources. For servers with `merge=false`,
/// mailbox names are prefixed with the service name (e.g., "Work/INBOX").
pub async fn fetch_and_merge_mailboxes(
    configs: &[ServerConfig],
    query: &QueryParameters,
) -> std::result::Result<Vec<MailboxState>, String> {
    use std::collections::HashMap;

    let mut merged: HashMap<String, Mailbox> = HashMap::new();
    let mut origins: HashMap<String, HashMap<MessageID, Vec<ToolOrigin>>> = HashMap::new();
    let mut message_indexes: HashMap<String, HashMap<MessageID, usize>> = HashMap::new();
    let mut errors = Vec::new();

    for config in configs {
        let origin = ToolOrigin::from_server_config(config);
        match fetch_mailboxes(&config.base_url, query.clone()).await {
            Ok(mailboxes) => {
                for mailbox in mailboxes {
                    let name = if config.merge {
                        mailbox.name.as_str().to_string()
                    } else {
                        format!("{}/{}", config.name, mailbox.name.as_str())
                    };

                    let origin_map = origins.entry(name.clone()).or_default();
                    if let Some(existing_mailbox) = merged.get_mut(&name) {
                        let index_map = message_indexes.entry(name.clone()).or_default();
                        for msg in mailbox.messages {
                            if let Some(existing_idx) = index_map.get(&msg.msg_id).copied() {
                                let existing_msg: &mut Message =
                                    &mut existing_mailbox.messages[existing_idx];
                                if !msg.tools.is_empty() {
                                    let entry = origin_map.entry(msg.msg_id.clone()).or_default();
                                    entry.extend(std::iter::repeat_n(
                                        origin.clone(),
                                        msg.tools.len(),
                                    ));
                                    existing_msg.tools.extend(msg.tools);
                                }
                            } else {
                                let new_idx = existing_mailbox.messages.len();
                                index_map.insert(msg.msg_id.clone(), new_idx);
                                let tool_origins =
                                    std::iter::repeat_n(origin.clone(), msg.tools.len())
                                        .collect::<Vec<_>>();
                                origin_map.insert(msg.msg_id.clone(), tool_origins);
                                existing_mailbox.messages.push(msg);
                            }
                        }
                    } else {
                        let mut index_map = HashMap::new();
                        let mut messages: Vec<Message> = Vec::new();
                        for msg in mailbox.messages {
                            if let Some(existing_idx) = index_map.get(&msg.msg_id).copied() {
                                let existing_msg: &mut Message = &mut messages[existing_idx];
                                if !msg.tools.is_empty() {
                                    let entry = origin_map.entry(msg.msg_id.clone()).or_default();
                                    entry.extend(std::iter::repeat_n(
                                        origin.clone(),
                                        msg.tools.len(),
                                    ));
                                    existing_msg.tools.extend(msg.tools);
                                }
                            } else {
                                let new_idx = messages.len();
                                index_map.insert(msg.msg_id.clone(), new_idx);
                                let tool_origins =
                                    std::iter::repeat_n(origin.clone(), msg.tools.len())
                                        .collect::<Vec<_>>();
                                origin_map.insert(msg.msg_id.clone(), tool_origins);
                                messages.push(msg);
                            }
                        }
                        message_indexes.insert(name.clone(), index_map);
                        // Create new mailbox with the (possibly prefixed) name.
                        let new_name = MailboxName::new(&name).unwrap_or(mailbox.name.clone());
                        let new_mailbox = Mailbox {
                            name: new_name,
                            messages,
                        };
                        merged.insert(name, new_mailbox);
                    }
                }
            }
            Err(e) => errors.push(format!("{}: {}", config.base_url, e)),
        }
    }

    if merged.is_empty() && !errors.is_empty() {
        Err(errors.join("; "))
    } else {
        // Sort mailboxes by name for consistent ordering.
        let mut items: Vec<(String, Mailbox)> = merged.into_iter().collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));

        let mailbox_states = items
            .into_iter()
            .map(|(name, mailbox)| {
                let origin_map = origins.remove(&name).unwrap_or_default();
                MailboxState::new_with_origins(mailbox, origin_map)
            })
            .collect();

        Ok(mailbox_states)
    }
}

/// Polls and handles pending background fetch responses.
pub fn poll_pending_fetch(
    windows: &mut Windows,
    pending_fetch: &mut Option<Receiver<std::result::Result<Vec<MailboxState>, String>>>,
) {
    let primary_dialog_active = is_primary_dialog_active(windows);
    if let Some(ref rx) = pending_fetch {
        match rx.try_recv() {
            Ok(Ok(mailbox_states)) => {
                if let Some(state) = windows.state_mut() {
                    state.update_mailboxes(mailbox_states, primary_dialog_active);
                    if primary_dialog_active {
                        state.status_message = "Refreshed mailboxes".to_string();
                        state.dirty.set_message_dirty(true);
                    }
                }
                *pending_fetch = None;
            }
            Ok(Err(e)) => {
                if let Some(state) = windows.state_mut() {
                    state.status_message = format!("Refresh failed: {}", e);
                    state.dirty.set_message_dirty(true);
                }
                *pending_fetch = None;
            }
            Err(TryRecvError::Empty) => {
                // Still fetching, continue.
            }
            Err(TryRecvError::Disconnected) => {
                // Thread died unexpectedly.
                if let Some(state) = windows.state_mut() {
                    state.status_message = "Refresh failed: worker disconnected".to_string();
                    state.dirty.set_message_dirty(true);
                }
                *pending_fetch = None;
            }
        }
    }
}
