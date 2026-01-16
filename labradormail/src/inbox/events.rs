use std::io::Result;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;

use crate::agent_dialog::agent_dialog_handle_key;
use crate::global::dialog_default_bindings;
use crate::global::generic_default_bindings;
use crate::global::global_function_dispatcher_active;
use crate::global::lookup_binding;
use crate::global::FunctionRetval;
use crate::window::WindowType;

use super::background::spawn_auto_complete;
use super::background::spawn_tool_call;
use super::state::tool_display_name;
use super::state::InboxState;
use super::state::ToolConflictChoice;
use super::state::ToolConflictState;
use super::state::ToolFormState;
use super::state::ToolOrigin;
use super::ui::Windows;
use agent_inbox_protocol::ToolCallRequest;
use serde_json::Value;

/// Result of handling a key event.
pub enum KeyHandleResult {
    Continue,
    Abort,
}

pub enum ToolFormAction {
    None,
    Submit(ToolCallRequest, String, String),
    Cancel,
    StartAutoComplete,
    RegenerateField,
}

fn apply_tool_prefill(form: &mut ToolFormState, input: &Value) {
    let Value::Object(input_obj) = input else {
        return;
    };
    for field in &mut form.fields {
        if let Some(val) = input_obj.get(&field.name) {
            field.value = match val {
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                _ => serde_json::to_string(val).unwrap_or_default(),
            };
        }
    }
}

#[derive(Clone)]
struct ToolSelection {
    tool: claudius::ToolParam,
    origin: Option<ToolOrigin>,
    indexed: bool,
    display_name: String,
}

enum ToolSelectionResult {
    None,
    Selected(ToolSelection),
    Conflict(ToolConflictState),
}

fn select_tool_for_key(
    message: &agent_inbox_protocol::Message,
    origins: Option<&[ToolOrigin]>,
    ch: char,
) -> ToolSelectionResult {
    if ch.is_ascii_digit() {
        let idx = ch.to_digit(10).unwrap_or(0);
        if idx == 0 {
            return ToolSelectionResult::None;
        }
        let index = (idx - 1) as usize;
        return message
            .tools
            .get(index)
            .cloned()
            .map(|tool| {
                let origin = origins
                    .and_then(|origin_list| origin_list.get(index))
                    .cloned();
                let display_name = tool_display_name(&tool.name, origin.as_ref());
                ToolSelection {
                    tool,
                    origin,
                    indexed: true,
                    display_name,
                }
            })
            .map_or(ToolSelectionResult::None, ToolSelectionResult::Selected);
    }

    let matches: Vec<ToolSelection> = message
        .tools
        .iter()
        .enumerate()
        .filter_map(|(idx, tool)| {
            let origin = origins
                .and_then(|origin_list| origin_list.get(idx))
                .cloned();
            let display_name = tool_display_name(&tool.name, origin.as_ref());
            let matches = display_name
                .chars()
                .next()
                .map(|c| c.eq_ignore_ascii_case(&ch))
                .unwrap_or(false);
            if matches {
                Some(ToolSelection {
                    tool: tool.clone(),
                    origin,
                    indexed: false,
                    display_name,
                })
            } else {
                None
            }
        })
        .collect();

    if matches.is_empty() {
        return ToolSelectionResult::None;
    }
    if matches.len() == 1 {
        return ToolSelectionResult::Selected(matches[0].clone());
    }

    let tool_name = matches[0].display_name.clone();
    if matches
        .iter()
        .all(|selection| selection.display_name == tool_name)
    {
        let choices: Vec<ToolConflictChoice> = matches
            .into_iter()
            .filter_map(|selection| {
                selection.origin.map(|origin| ToolConflictChoice {
                    tool: selection.tool,
                    origin,
                })
            })
            .collect();
        return ToolSelectionResult::Conflict(ToolConflictState {
            message_id: message.msg_id.clone(),
            tool_name,
            choices,
            prefill: None,
        });
    }

    ToolSelectionResult::None
}

fn try_open_tool_form(state: &mut InboxState, key: KeyEvent) -> bool {
    if state.tool_form.is_some() || state.tool_conflict.is_some() {
        return false;
    }
    let KeyCode::Char(ch) = key.code else {
        return false;
    };
    let (selection, origins, message_id, message_tools) = {
        let Some(message) = state.selected_message() else {
            return false;
        };
        let message_id = message.msg_id.clone();
        let message_tools = message.tools.clone();
        let origins = state
            .current_mailbox()
            .and_then(|mb| mb.tool_origins_for_message(&message_id));
        let selection = select_tool_for_key(message, origins, ch);
        (selection, origins, message_id, message_tools)
    };
    match selection {
        ToolSelectionResult::None => false,
        ToolSelectionResult::Selected(selection) => {
            let tool_name = selection.display_name.clone();
            if !selection.indexed {
                let choices: Vec<ToolConflictChoice> = message_tools
                    .iter()
                    .enumerate()
                    .filter(|(idx, tool)| {
                        let origin = origins.and_then(|origin_list| origin_list.get(*idx));
                        tool_display_name(&tool.name, origin) == tool_name
                    })
                    .filter_map(|(idx, tool)| {
                        origins
                            .and_then(|origin_list| origin_list.get(idx))
                            .cloned()
                            .map(|origin| ToolConflictChoice {
                                tool: tool.clone(),
                                origin,
                            })
                    })
                    .collect();
                if choices.len() > 1 {
                    let options = choices
                        .iter()
                        .enumerate()
                        .map(|(idx, choice)| format!("{}: {}", idx + 1, choice.origin.label))
                        .collect::<Vec<String>>()
                        .join(", ");
                    state.status_message = format!(
                        "Multiple '{}' tools: {} (press 1-{} or Esc)",
                        tool_name,
                        options,
                        choices.len()
                    );
                    state.tool_conflict = Some(ToolConflictState {
                        message_id,
                        tool_name,
                        choices,
                        prefill: None,
                    });
                    state.dirty.mark_message_views();
                    state.dirty.set_message_dirty(true);
                    return true;
                }
            }
            if selection.origin.is_none() {
                state.status_message = "Tool origin unknown".to_string();
                state.dirty.set_message_dirty(true);
                return true;
            }
            state.tool_form = Some(ToolFormState::new(
                message_id,
                selection.tool,
                selection.origin,
            ));
            state.status_message = format!(
                "Tool: {} (Tab/Shift-Tab move, Ctrl-Space auto-fill, Ctrl-R regen, Enter submit)",
                tool_name
            );
            state.dirty.mark_message_views();
            state.dirty.set_message_dirty(true);
            true
        }
        ToolSelectionResult::Conflict(conflict) => {
            if conflict.choices.len() <= 1 {
                state.status_message = "Tool origin unknown".to_string();
                state.dirty.set_message_dirty(true);
                return true;
            }
            let options = conflict
                .choices
                .iter()
                .enumerate()
                .map(|(idx, choice)| format!("{}: {}", idx + 1, choice.origin.label))
                .collect::<Vec<String>>()
                .join(", ");
            state.status_message = format!(
                "Multiple '{}' tools: {} (press 1-{} or Esc)",
                conflict.tool_name,
                options,
                conflict.choices.len()
            );
            state.tool_conflict = Some(conflict);
            state.dirty.mark_message_views();
            state.dirty.set_message_dirty(true);
            true
        }
    }
}

pub fn handle_tool_form_key(state: &mut InboxState, key: KeyEvent) -> ToolFormAction {
    let Some(form) = state.tool_form.as_mut() else {
        return ToolFormAction::None;
    };

    // Check if auto-complete is in progress
    let auto_completing = form.is_auto_completing();
    let is_shift_tab = matches!(key.code, KeyCode::BackTab)
        || (matches!(key.code, KeyCode::Tab) && key.modifiers.contains(KeyModifiers::SHIFT));
    let is_ctrl_space =
        matches!(key.code, KeyCode::Char(' ')) && key.modifiers.contains(KeyModifiers::CONTROL);
    let is_ctrl_r =
        matches!(key.code, KeyCode::Char('r')) && key.modifiers.contains(KeyModifiers::CONTROL);

    if is_shift_tab {
        if !auto_completing {
            form.focus_prev();
            state.dirty.set_pager_dirty(true);
        }
    } else if is_ctrl_space {
        // Ctrl-Space triggers auto-complete
        if !auto_completing {
            return ToolFormAction::StartAutoComplete;
        }
    } else if is_ctrl_r {
        if !auto_completing {
            return ToolFormAction::RegenerateField;
        }
    } else {
        match key.code {
            KeyCode::Tab => {
                if !auto_completing {
                    form.focus_next();
                    state.dirty.set_pager_dirty(true);
                }
            }
            KeyCode::Esc => {
                if auto_completing {
                    form.cancel_auto_complete();
                    state.status_message = "Auto-fill cancelled".to_string();
                    state.dirty.set_message_dirty(true);
                } else {
                    state.tool_form = None;
                    state.status_message = "Tool canceled".to_string();
                    state.dirty.mark_message_views();
                    state.dirty.set_message_dirty(true);
                    return ToolFormAction::Cancel;
                }
            }
            KeyCode::Enter => {
                if auto_completing {
                    form.apply_auto_complete();
                    state.status_message = "Auto-fill accepted".to_string();
                    state.dirty.set_pager_dirty(true);
                    state.dirty.set_message_dirty(true);
                } else if form.is_submit_focused() {
                    match form.build_input() {
                        Ok(input) => {
                            let Some(origin) = form.origin.as_ref() else {
                                state.status_message = "Tool origin unknown".to_string();
                                state.dirty.set_message_dirty(true);
                                return ToolFormAction::None;
                            };
                            let request = ToolCallRequest {
                                message_id: form.message_id.clone(),
                                name: form.tool.name.clone(),
                                input,
                            };
                            let tool_name =
                                tool_display_name(&form.tool.name, form.origin.as_ref());
                            let base_url = origin.base_url.clone();
                            state.tool_form = None;
                            state.dirty.mark_message_views();
                            state.dirty.set_message_dirty(true);
                            return ToolFormAction::Submit(request, tool_name, base_url);
                        }
                        Err(err) => {
                            state.status_message = format!("Tool input error: {}", err);
                            state.dirty.set_message_dirty(true);
                        }
                    }
                } else if form.is_cancel_focused() {
                    state.tool_form = None;
                    state.status_message = "Tool canceled".to_string();
                    state.dirty.mark_message_views();
                    state.dirty.set_message_dirty(true);
                    return ToolFormAction::Cancel;
                }
            }
            KeyCode::Backspace => {
                // Cancel auto-complete if in progress
                if auto_completing {
                    form.cancel_auto_complete();
                    state.status_message = "Auto-fill cancelled".to_string();
                    state.dirty.set_message_dirty(true);
                } else if let Some(field) = form.focused_field_mut() {
                    field.value.pop();
                    state.dirty.set_pager_dirty(true);
                }
            }
            KeyCode::Char(ch) => {
                if !auto_completing {
                    if let Some(field) = form.focused_field_mut() {
                        field.value.push(ch);
                        state.dirty.set_pager_dirty(true);
                    }
                }
            }
            _ => {}
        }
    }

    ToolFormAction::None
}

fn handle_tool_conflict_key(state: &mut InboxState, key: KeyEvent) -> bool {
    let Some(conflict) = state.tool_conflict.take() else {
        return false;
    };

    match key.code {
        KeyCode::Esc => {
            state.status_message = "Tool selection canceled".to_string();
            state.dirty.set_message_dirty(true);
            true
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() => {
            let idx = ch.to_digit(10).unwrap_or(0);
            let choices_len = conflict.choices.len();
            if idx == 0 || idx as usize > choices_len {
                state.status_message = format!("Choose 1-{} or Esc to cancel", choices_len);
                state.tool_conflict = Some(conflict);
                state.dirty.set_message_dirty(true);
                return true;
            }
            let choice = conflict.choices[(idx - 1) as usize].clone();
            let display_name = tool_display_name(&choice.tool.name, Some(&choice.origin));
            let mut form = ToolFormState::new(
                conflict.message_id,
                choice.tool.clone(),
                Some(choice.origin),
            );
            if let Some(prefill) = &conflict.prefill {
                apply_tool_prefill(&mut form, prefill);
            }
            state.tool_form = Some(form);
            state.status_message = format!(
                "Tool: {} (Tab/Shift-Tab move, Ctrl-Space auto-fill, Ctrl-R regen, Enter submit)",
                display_name
            );
            state.dirty.mark_message_views();
            state.dirty.set_message_dirty(true);
            true
        }
        _ => {
            state.tool_conflict = Some(conflict);
            true
        }
    }
}

/// Handles a resize event.
pub fn handle_resize_event(windows: &mut Windows, cols: u16, rows: u16) -> Result<()> {
    windows.handle_resize()?;
    if let Some(state) = windows.state_mut() {
        state.status_message = format!("Resized to {}x{}", cols, rows);
        state.dirty.mark_all();
    }
    Ok(())
}

/// Handles a key event, returning whether to continue or abort the event loop.
pub fn handle_key_event(windows: &mut Windows, key: KeyEvent) -> KeyHandleResult {
    let (has_tool_form, has_tool_confirm, _tool_prefix_active) = windows
        .state()
        .map(|state| {
            (
                state.tool_form.is_some(),
                state.tool_confirm_pending.is_some(),
                state.tool_prefix_active,
            )
        })
        .unwrap_or((false, false, false));
    // Handle tool call confirmation prompt (y/n/Esc).
    if has_tool_confirm {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(state) = windows.state_mut() {
                    if let Some(confirmation) = state.tool_confirm_pending.take() {
                        state.status_message =
                            format!("Executing tool '{}'...", confirmation.tool_name);
                        state.dirty.set_message_dirty(true);
                        state.tool_call_pending =
                            Some(spawn_tool_call(confirmation.base_url, confirmation.request));
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                if let Some(state) = windows.state_mut() {
                    if let Some(confirmation) = state.tool_confirm_pending.take() {
                        state.status_message =
                            format!("Tool '{}' execution cancelled", confirmation.tool_name);
                        state.dirty.set_message_dirty(true);
                    }
                }
            }
            _ => {
                // Ignore other keys, keep waiting for y/n/Esc
            }
        }
        return KeyHandleResult::Continue;
    }

    if has_tool_form {
        let action = match windows.state_mut() {
            Some(state) => handle_tool_form_key(state, key),
            None => return KeyHandleResult::Continue,
        };
        match action {
            ToolFormAction::Submit(request, tool_name, base_url) => {
                if let Some(state) = windows.state_mut() {
                    state.status_message = format!("Executing tool '{}'...", tool_name);
                    state.tool_call_pending = Some(spawn_tool_call(base_url, request));
                    state.dirty.set_message_dirty(true);
                }
            }
            ToolFormAction::StartAutoComplete => {
                if let Some(state) = windows.state_mut() {
                    if let Some(ac_state) = spawn_auto_complete(state, false, true) {
                        if let Some(ref mut form) = state.tool_form {
                            form.auto_complete = Some(ac_state);
                        }
                        state.status_message =
                            "Auto-fill streaming... (Esc/Backspace cancel, Enter accept)"
                                .to_string();
                        state.dirty.set_message_dirty(true);
                    } else {
                        state.status_message = "Auto-fill unavailable".to_string();
                        state.dirty.set_message_dirty(true);
                    }
                }
            }
            ToolFormAction::RegenerateField => {
                if let Some(state) = windows.state_mut() {
                    if let Some(ac_state) = spawn_auto_complete(state, true, false) {
                        if let Some(ref mut form) = state.tool_form {
                            form.auto_complete = Some(ac_state);
                        }
                        state.status_message =
                            "Regenerating field... (Esc/Backspace cancel, Enter accept)"
                                .to_string();
                        state.dirty.set_message_dirty(true);
                    } else {
                        state.status_message = "Regenerate unavailable".to_string();
                        state.dirty.set_message_dirty(true);
                    }
                }
            }
            ToolFormAction::None | ToolFormAction::Cancel => {}
        }
        return KeyHandleResult::Continue;
    }

    let has_tool_conflict = windows
        .state()
        .is_some_and(|state| state.tool_conflict.is_some());
    if has_tool_conflict {
        let handled = match windows.state_mut() {
            Some(state) => handle_tool_conflict_key(state, key),
            None => false,
        };
        if handled {
            return KeyHandleResult::Continue;
        }
    }

    // Check if agent dialog is active and handle key input first.
    {
        let all_dialogs = windows.root.all_dialogs_id();
        let tree = windows.root.tree_mut();
        if let Some(top) = tree.stack_top(all_dialogs) {
            if tree.get(top).window_type == WindowType::DlgAgent
                && agent_dialog_handle_key(tree, top, key)
            {
                return KeyHandleResult::Continue;
            }
        }
    }

    let tool_prefix_active = windows
        .state()
        .is_some_and(|state| state.tool_prefix_active);
    if tool_prefix_active {
        if let Some(state) = windows.state_mut() {
            state.tool_prefix_active = false;
            match key.code {
                KeyCode::Esc => {
                    state.status_message = "Tool selection canceled".to_string();
                    state.dirty.set_message_dirty(true);
                }
                KeyCode::Char(_) => {
                    if !try_open_tool_form(state, key) {
                        state.status_message = "No tool matches that key".to_string();
                        state.dirty.set_message_dirty(true);
                    }
                }
                _ => {
                    state.status_message = "Tool selection canceled".to_string();
                    state.dirty.set_message_dirty(true);
                }
            }
        }
        return KeyHandleResult::Continue;
    }

    if let KeyCode::Char('T') = key.code {
        if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
            if let Some(state) = windows.state_mut() {
                state.tool_prefix_active = true;
                state.status_message = "Tool: press letter/number (Esc to cancel)".to_string();
                state.dirty.set_message_dirty(true);
            }
            return KeyHandleResult::Continue;
        }
    }

    // Global bindings always take precedence over tool shortcuts.
    let op = lookup_binding(&dialog_default_bindings(), key)
        .or_else(|| lookup_binding(&generic_default_bindings(), key));
    if let Some(op) = op {
        let ret = global_function_dispatcher_active(
            windows.root.main_layout,
            windows.root.tree_mut(),
            &mut windows.ctx,
            op,
            &windows.global_functions,
        );
        if ret == FunctionRetval::Abort {
            return KeyHandleResult::Abort;
        }
        if ret != FunctionRetval::Unhandled {
            if let Some(state) = windows.state_mut() {
                state.dirty.set_help_bar_dirty(true);
            }
        }
        return KeyHandleResult::Continue;
    }

    // Only try tool shortcuts if no global binding matched.
    if let Some(state) = windows.state_mut() {
        if try_open_tool_form(state, key) {
            state.dirty.set_help_bar_dirty(true);
        }
    }
    KeyHandleResult::Continue
}
