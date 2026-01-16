use std::io::Result;
use std::io::Stdout;
use std::time::Duration;

use crossterm::event::poll;
use crossterm::event::read;
use crossterm::event::Event;

use crate::agent_dialog::agent_dialog_poll;
use crate::global::Bindings;
use crate::root_window::RootWindowConfig;
use crate::window::WindowType;

use super::background::poll_pending_auto_complete;
use super::background::poll_pending_fetch;
use super::background::poll_pending_tool_call;
use super::background::spawn_background_fetch;
use super::events::handle_key_event;
use super::events::handle_resize_event;
use super::events::KeyHandleResult;
use super::state::MailboxState;
use super::state::ServerConfig;
use super::ui::current_dialog_top;
use super::ui::redraw;
use super::ui::Windows;
use agent_inbox_protocol::Mailbox;

/// Interval between background refresh attempts.
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Poll timeout for event loop.
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Polls agent dialog for streaming updates if it's the active dialog.
fn poll_agent_dialog(windows: &mut Windows) {
    let all_dialogs = windows.root.all_dialogs_id();
    let tree = windows.root.tree_mut();
    if let Some(top) = tree.stack_top(all_dialogs) {
        if tree.get(top).window_type == WindowType::DlgAgent {
            agent_dialog_poll(tree, top);
        }
    }
}

/// Handles a crossterm event, returning whether to continue or abort the event loop.
fn handle_event(windows: &mut Windows, event: Event) -> Result<KeyHandleResult> {
    match event {
        Event::Key(key) => Ok(handle_key_event(windows, key)),
        Event::Resize(cols, rows) => {
            handle_resize_event(windows, cols, rows)?;
            Ok(KeyHandleResult::Continue)
        }
        Event::Mouse(_) | Event::FocusGained | Event::FocusLost | Event::Paste(_) => {
            Ok(KeyHandleResult::Continue)
        }
    }
}

pub fn run_loop(
    stdout: &mut Stdout,
    mailboxes: Vec<Mailbox>,
    config: RootWindowConfig,
    dialog_bindings: Bindings,
    global_bindings: Bindings,
) -> Result<()> {
    let mut windows = Windows::new(stdout, mailboxes, config, dialog_bindings, global_bindings)?;

    // Initial draw.
    redraw(stdout, &mut windows)?;

    loop {
        let dialog_before = current_dialog_top(&windows);

        poll_pending_tool_call(&mut windows);
        poll_pending_auto_complete(&mut windows);
        poll_agent_dialog(&mut windows);

        let tool_call_pending = windows
            .state()
            .is_some_and(|s| s.tool_call_pending.is_some());
        let tool_confirm_pending = windows
            .state()
            .is_some_and(|s| s.tool_confirm_pending.is_some());

        // Always use polling when agent dialog might be streaming
        if tool_call_pending || tool_confirm_pending {
            // Moderate polling when tool operations pending
            if poll(POLL_TIMEOUT)? {
                if let KeyHandleResult::Abort = handle_event(&mut windows, read()?)? {
                    break;
                }
            }
        } else if poll(POLL_TIMEOUT)? {
            // Poll with timeout to allow streaming updates
            if let KeyHandleResult::Abort = handle_event(&mut windows, read()?)? {
                break;
            }
        }

        // Check if dialog state changed (any dialog push/pop)
        if dialog_before != current_dialog_top(&windows) {
            if let Some(state) = windows.state_mut() {
                state.dirty.mark_all();
            }
        }

        // Redraw only what changed.
        redraw(stdout, &mut windows)?;
    }

    Ok(())
}

/// Run the event loop with periodic background refresh from multiple servers.
pub fn run_loop_with_refresh(
    stdout: &mut Stdout,
    mailbox_states: Vec<MailboxState>,
    configs: &[ServerConfig],
    config: RootWindowConfig,
    dialog_bindings: Bindings,
    global_bindings: Bindings,
) -> Result<()> {
    let mut windows = Windows::new_with_states(
        stdout,
        mailbox_states,
        config,
        dialog_bindings,
        global_bindings,
    )?;

    // Initial draw.
    redraw(stdout, &mut windows)?;

    // Track background fetch state.
    use std::sync::mpsc::Receiver;
    let mut pending_fetch: Option<Receiver<std::result::Result<Vec<MailboxState>, String>>> = None;
    let mut last_refresh = std::time::Instant::now();

    loop {
        let dialog_before = current_dialog_top(&windows);

        poll_pending_tool_call(&mut windows);
        poll_pending_fetch(&mut windows, &mut pending_fetch);
        poll_pending_auto_complete(&mut windows);
        poll_agent_dialog(&mut windows);

        let mut refresh_requested = false;
        let mut tool_ui_active = false;
        if let Some(state) = windows.state_mut() {
            tool_ui_active = state.tool_form.is_some()
                || state.tool_confirm_pending.is_some()
                || state.tool_call_pending.is_some()
                || state.tool_prefix_active;
            if state.refresh_requested {
                refresh_requested = true;
                if !tool_ui_active {
                    state.refresh_requested = false;
                }
            }
        }

        // Start a new background fetch if it's time, none is pending, and no tool UI is active.
        if !tool_ui_active
            && pending_fetch.is_none()
            && (refresh_requested || last_refresh.elapsed() >= REFRESH_INTERVAL)
        {
            if let Some(state) = windows.state() {
                let query = state.server_query.clone();
                pending_fetch = Some(spawn_background_fetch(configs.to_vec(), query));
                last_refresh = std::time::Instant::now();
            }
        }

        // Poll for events with a timeout so we can check background fetches.
        if poll(POLL_TIMEOUT)? {
            if let KeyHandleResult::Abort = handle_event(&mut windows, read()?)? {
                break;
            }
        }

        // Check if dialog state changed (any dialog push/pop)
        if dialog_before != current_dialog_top(&windows) {
            if let Some(state) = windows.state_mut() {
                state.dirty.mark_all();
            }
        }

        // Redraw only what changed.
        redraw(stdout, &mut windows)?;
    }

    Ok(())
}
