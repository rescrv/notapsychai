use crossterm::cursor::MoveTo;

use super::state::InboxState;
use crate::action::Action;
use crate::agent_dialog::agent_dialog_content_window;
use crate::agent_dialog::AgentDialog;
use crate::command::parse_command;
use crate::command::CommandAction;
use crate::context::GuiContext;
use crate::curs_lib::mw_enter_fname;
use crate::curs_lib::FileCompletionData;
use crate::curs_lib::SelectFileFlags;
use crate::global::default_global_functions;
use crate::global::FunctionRetval;
use crate::global::GlobalFunctionEntry;
use crate::window::window_status_on_top;
use crate::window::WindowId;
use crate::window::WindowTree;
use crate::window::WindowType;
use crate::window::WindowWidget;

fn inbox_state_mut(tree: &mut WindowTree, win: WindowId) -> Option<&mut InboxState> {
    if let Some(WindowWidget::InboxState(state)) = tree.get_mut(win).widget_mut() {
        Some(state.as_mut())
    } else {
        None
    }
}

fn with_inbox_state(
    tree: &mut WindowTree,
    win: WindowId,
    f: impl FnOnce(&mut InboxState),
) -> FunctionRetval {
    if let Some(state) = inbox_state_mut(tree, win) {
        f(state);
        FunctionRetval::Done
    } else {
        FunctionRetval::Unhandled
    }
}

/// Global function handler for NextEntry (j/Down).
fn op_next_entry(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    with_inbox_state(tree, win, |state| state.select_next_message())
}

/// Global function handler for PrevEntry (k/Up).
fn op_prev_entry(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    with_inbox_state(tree, win, |state| state.select_prev_message())
}

/// Global function handler for SidebarNext (J).
fn op_sidebar_next(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    with_inbox_state(tree, win, |state| state.select_next_mailbox())
}

/// Global function handler for SidebarPrev (K).
fn op_sidebar_prev(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    with_inbox_state(tree, win, |state| state.select_prev_mailbox())
}

/// Global function handler for EnterCommand (:).
fn op_enter_command(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    // We need to access the message window which is usually at the root level.
    // Since we don't have direct access to MessageWindow here, we'll just use stdout directly
    // and rely on mw_enter_fname to handle the UI at the cursor position.

    let mut stdout = std::io::stdout();
    // Move to bottom left
    let (_, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let _ = crossterm::queue!(stdout, MoveTo(0, rows - 1));

    let mut command = String::new();
    let mut completion = FileCompletionData::default();

    // Temporarily show cursor
    let _ = crossterm::execute!(stdout, crossterm::cursor::Show);

    let ret = mw_enter_fname(
        &mut stdout,
        ":",
        &mut command,
        &mut completion,
        SelectFileFlags::NONE,
    );

    let _ = crossterm::execute!(stdout, crossterm::cursor::Hide);

    if let Ok(0) = ret {
        match parse_command(&command) {
            CommandAction::Op(Action::Quit) => return FunctionRetval::Abort,
            CommandAction::Set(key, value) => {
                if key == "status_on_top" {
                    let root = tree.get_root(win);
                    let is_top = value == "yes" || value == "true" || value == "1";
                    window_status_on_top(tree, root, is_top);

                    // Update status message
                    if let Some(state) = inbox_state_mut(tree, win) {
                        state.status_message = format!("Set status_on_top = {}", is_top);
                        state.dirty.set_message_dirty(true);
                    }
                    return FunctionRetval::Done;
                }
                if let Some(state) = inbox_state_mut(tree, win) {
                    state.status_message = format!("Unknown option: {}", key);
                    state.dirty.set_message_dirty(true);
                }
            }
            CommandAction::Op(action) => {
                if let Some(state) = inbox_state_mut(tree, win) {
                    state.status_message = format!("Command action: {:?}", action);
                    state.dirty.set_message_dirty(true);
                }
            }
            CommandAction::Unknown(cmd) => {
                if let Some(state) = inbox_state_mut(tree, win) {
                    state.status_message = format!("Unknown command: {}", cmd);
                    state.dirty.set_message_dirty(true);
                }
            }
            CommandAction::Empty => {}
        }
    }

    FunctionRetval::Done
}

/// Global function handler for Search (/).
fn op_search(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    let mut stdout = std::io::stdout();
    let (_, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let _ = crossterm::queue!(stdout, MoveTo(0, rows - 1));

    let mut pattern = String::new();
    let mut completion = FileCompletionData::default();

    let _ = crossterm::execute!(stdout, crossterm::cursor::Show);

    let ret = mw_enter_fname(
        &mut stdout,
        "/",
        &mut pattern,
        &mut completion,
        SelectFileFlags::NONE,
    );

    let _ = crossterm::execute!(stdout, crossterm::cursor::Hide);

    if let Ok(0) = ret {
        if let Some(state) = inbox_state_mut(tree, win) {
            if pattern.is_empty() {
                state.server_query.search = None;
                state.status_message = "Search cleared".to_string();
            } else {
                state.server_query.search = Some(pattern.clone());
                state.status_message = format!("Searching for: {}", pattern);
            }
            state.refresh_requested = true;
            state.dirty.set_message_dirty(true);
        }
    }

    FunctionRetval::Done
}

/// Global function handler for Mail (m).
fn op_mail(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    let mut stdout = std::io::stdout();
    let (_, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let _ = crossterm::queue!(stdout, MoveTo(0, rows - 1));

    let mut to_addr = String::new();
    let mut subject = String::new();
    let mut completion = FileCompletionData::default();

    let _ = crossterm::execute!(stdout, crossterm::cursor::Show);

    if let Ok(0) = mw_enter_fname(
        &mut stdout,
        "To:",
        &mut to_addr,
        &mut completion,
        SelectFileFlags::NONE,
    ) {
        if let Ok(0) = mw_enter_fname(
            &mut stdout,
            "Subject:",
            &mut subject,
            &mut completion,
            SelectFileFlags::NONE,
        ) {
            if let Some(state) = inbox_state_mut(tree, win) {
                state.status_message =
                    format!("Sent mail to '{}' with subject '{}'", to_addr, subject);
                state.dirty.set_message_dirty(true);
            }
        }
    }

    let _ = crossterm::execute!(stdout, crossterm::cursor::Hide);

    FunctionRetval::Done
}

/// Global function handler for Agent (a).
fn op_agent(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    let root = tree.get_root(win);
    let Some(all_dialogs) = tree.find_child(root, WindowType::AllDialogs) else {
        return FunctionRetval::Unhandled;
    };

    // Check if agent dialog already exists in stack
    let existing = tree
        .get(all_dialogs)
        .children
        .iter()
        .find(|&&child| tree.get(child).window_type == WindowType::DlgAgent)
        .copied();

    if let Some(agent_dialog) = existing {
        // Bring existing dialog to top
        tree.stack_bring_to_top(all_dialogs, agent_dialog);
        let focus_target = agent_dialog_content_window(tree, agent_dialog).unwrap_or(agent_dialog);
        tree.set_focus(focus_target);
        return FunctionRetval::Done;
    }

    // Create new agent dialog
    let dialog = AgentDialog::new(tree);
    tree.stack_push(all_dialogs, dialog.window_id());
    let focus_target =
        agent_dialog_content_window(tree, dialog.window_id()).unwrap_or(dialog.window_id());
    tree.set_focus(focus_target);
    FunctionRetval::Done
}

/// Global function handler for YankMessage (y).
fn op_yank_message(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    // Get the currently selected message and its tool origin from inbox state
    let (message, origins) = if let Some(state) = inbox_state_mut(tree, win) {
        let msg = state.selected_message().cloned();
        let origins = msg.as_ref().and_then(|m| {
            state
                .current_mailbox()
                .and_then(|mb| mb.tool_origins_for_message(&m.msg_id))
                .map(|origins| origins.to_vec())
        });
        (msg, origins)
    } else {
        return FunctionRetval::Unhandled;
    };

    let Some(message) = message else {
        if let Some(state) = inbox_state_mut(tree, win) {
            state.status_message = "No message selected".to_string();
            state.dirty.set_message_dirty(true);
        }
        return FunctionRetval::Done;
    };

    let root = tree.get_root(win);
    let Some(all_dialogs) = tree.find_child(root, WindowType::AllDialogs) else {
        return FunctionRetval::Unhandled;
    };

    // Find or create the agent dialog
    let agent_dialog = tree
        .get(all_dialogs)
        .children
        .iter()
        .find(|&&child| tree.get(child).window_type == WindowType::DlgAgent)
        .copied();

    let agent_dialog = match agent_dialog {
        Some(dialog) => {
            tree.stack_bring_to_top(all_dialogs, dialog);
            dialog
        }
        None => {
            let dialog = AgentDialog::new(tree);
            tree.stack_push(all_dialogs, dialog.window_id());
            dialog.window_id()
        }
    };

    // Find the content window within the agent dialog
    let content = tree
        .get(agent_dialog)
        .children
        .iter()
        .copied()
        .find(|child| tree.get(*child).window_type == WindowType::Custom);

    if let Some(content) = content {
        if let Some(WindowWidget::AgentDialog(data)) = tree.get_mut(content).widget_mut() {
            data.yank_message(message.clone(), origins);
            tree.get_mut(content).mark_repaint();
        }
    }

    // Update status message
    if let Some(state) = inbox_state_mut(tree, win) {
        let tool_count = message.tools.len();
        if tool_count > 0 {
            state.status_message = format!(
                "Yanked message from {} with {} tool(s)",
                message.from.as_str(),
                tool_count
            );
        } else {
            state.status_message = format!("Yanked message from {}", message.from.as_str());
        }
        state.dirty.set_message_dirty(true);
    }

    // Focus the agent dialog
    let focus_target = agent_dialog_content_window(tree, agent_dialog).unwrap_or(agent_dialog);
    tree.set_focus(focus_target);

    FunctionRetval::Done
}

/// Inbox-specific global functions for navigation.
pub fn inbox_global_functions() -> Vec<GlobalFunctionEntry> {
    let mut funcs: Vec<GlobalFunctionEntry> = default_global_functions().to_vec();
    funcs.extend([
        GlobalFunctionEntry {
            action: Action::NextEntry,
            function: op_next_entry,
        },
        GlobalFunctionEntry {
            action: Action::PrevEntry,
            function: op_prev_entry,
        },
        GlobalFunctionEntry {
            action: Action::SidebarNext,
            function: op_sidebar_next,
        },
        GlobalFunctionEntry {
            action: Action::SidebarPrev,
            function: op_sidebar_prev,
        },
        GlobalFunctionEntry {
            action: Action::EnterCommand,
            function: op_enter_command,
        },
        GlobalFunctionEntry {
            action: Action::Search,
            function: op_search,
        },
        GlobalFunctionEntry {
            action: Action::Mail,
            function: op_mail,
        },
        GlobalFunctionEntry {
            action: Action::Agent,
            function: op_agent,
        },
        GlobalFunctionEntry {
            action: Action::YankMessage,
            function: op_yank_message,
        },
    ]);
    funcs
}
