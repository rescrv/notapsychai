//! Inbox application showcasing the gui crate.
//!
//! This module creates a typical neomutt-like layout with:
//! - Help bar at the top
//! - Sidebar on the left
//! - Index/Pager split in the center
//! - Status bars for each panel
//! - Message window at the bottom
//!
//! Use arrow keys to navigate, '?' for help, 'q' to quit.
//!
//! This application is entirely event-driven with no flickering - it only redraws
//! when something changes and uses selective redrawing.

use std::io::stdout;
use std::io::Result;
use std::io::Stdout;
use std::panic;

use crate::root_window::RootWindow;
use agent_inbox_protocol::Client;
use agent_inbox_protocol::Mailbox;
use agent_inbox_protocol::MailboxProvider;
use agent_inbox_protocol::Message;
use agent_inbox_protocol::MessageID;
use agent_inbox_protocol::QueryParameters;
use chrono::Utc;

use crate::global::dialog_default_bindings;
use crate::global::generic_default_bindings;
use crate::global::Bindings;
use crate::root_window::RootWindowConfig;

mod background;
mod commands;
mod controller;
mod events;
mod state;
mod ui;
mod view;

pub use state::tool_display_name;
pub use state::InboxState;
pub use state::ServerConfig;
pub use state::ToolOrigin;
pub use view::IndexBarWidget;
pub use view::IndexWidget;
pub use view::MessageWidget;
pub use view::PagerBarWidget;
pub use view::PagerWidget;
pub use view::SidebarWidget;

use background::fetch_and_merge_mailboxes;
use controller::run_loop;
use controller::run_loop_with_refresh;

/// Creates the default sample mailbox data.
pub fn sample_mailbox_data() -> Vec<Mailbox> {
    use agent_inbox_protocol::Body;
    use agent_inbox_protocol::From;
    use agent_inbox_protocol::MailboxName;

    let inbox = Mailbox {
        name: MailboxName::new("INBOX").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageID::default(),
                date: Utc::now(),
                from: From::new("alice@example.com").expect("valid from"),
                body: Body::new("Hello World").expect("valid body"),
                wrap: false,
                tools: vec![],
            },
            Message {
                msg_id: MessageID::default(),
                date: Utc::now(),
                from: From::new("bob@example.com").expect("valid from"),
                body: Body::new("Re: Hello World").expect("valid body"),
                wrap: false,
                tools: vec![],
            },
            Message {
                msg_id: MessageID::default(),
                date: Utc::now(),
                from: From::new("charlie@example.com").expect("valid from"),
                body: Body::new("Project Update").expect("valid body"),
                wrap: false,
                tools: vec![],
            },
        ],
    };

    let sent = Mailbox {
        name: MailboxName::new("Sent").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageID::default(),
                date: Utc::now(),
                from: From::new("me@example.com").expect("valid from"),
                body: Body::new("Re: Hello World").expect("valid body"),
                wrap: false,
                tools: vec![],
            },
            Message {
                msg_id: MessageID::default(),
                date: Utc::now(),
                from: From::new("me@example.com").expect("valid from"),
                body: Body::new("Meeting Tomorrow").expect("valid body"),
                wrap: false,
                tools: vec![],
            },
        ],
    };

    let drafts = Mailbox {
        name: MailboxName::new("Drafts").expect("valid mailbox name"),
        messages: vec![Message {
            msg_id: MessageID::default(),
            date: Utc::now(),
            from: From::new("me@example.com").expect("valid from"),
            body: Body::new("Draft: Proposal").expect("valid body"),
            wrap: false,
            tools: vec![],
        }],
    };

    let trash = Mailbox {
        name: MailboxName::new("Trash").expect("valid mailbox name"),
        messages: vec![Message {
            msg_id: MessageID::default(),
            date: Utc::now(),
            from: From::new("spam@example.com").expect("valid from"),
            body: Body::new("You've won!").expect("valid body"),
            wrap: false,
            tools: vec![],
        }],
    };

    vec![inbox, sent, drafts, trash]
}

/// Fetches mailboxes from an agent-inbox-protocol server.
///
/// This function connects to the specified base URL (e.g., `http://localhost:8080/inbox`)
/// and queries for all mailboxes using the provided query parameters.
pub async fn fetch_mailboxes(
    base_url: &str,
    query: QueryParameters,
) -> std::result::Result<Vec<Mailbox>, String> {
    let mut client = Client::new(base_url);
    let result = client
        .query(query)
        .await
        .map_err(|e| format!("HTTP query failed: {}", e))?;
    Ok(result.into_mailboxes())
}

/// Runs the provided function with panic hook and cleanup handling.
fn run_with_terminal_cleanup<F>(f: F) -> Result<()>
where
    F: FnOnce(&mut Stdout) -> Result<()>,
{
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let mut out = stdout();
        let _ = RootWindow::cleanup(&mut out);
        default_hook(info);
    }));

    let mut stdout = stdout();
    let result = f(&mut stdout);
    let _ = RootWindow::cleanup(&mut stdout);
    result
}

/// Run the inbox application with mailboxes from an agent-inbox-protocol server.
pub async fn run_with_mailboxes(mailboxes: Vec<Mailbox>) -> Result<()> {
    run_with_mailboxes_and_options(
        mailboxes,
        RootWindowConfig::default(),
        dialog_default_bindings(),
        generic_default_bindings(),
    )
    .await
}

/// Run the inbox application by fetching from multiple agent-inbox-protocol servers.
///
/// Fetches mailboxes initially from all servers, then periodically refreshes in the
/// background without blocking the event loop. Mailboxes from servers with `merge=true`
/// are combined at the top level, while servers with `merge=false` have their mailboxes
/// prefixed with the service name.
pub async fn run_from_servers(configs: &[ServerConfig]) -> Result<()> {
    let mailbox_states = fetch_and_merge_mailboxes(configs, &QueryParameters::default())
        .await
        .map_err(std::io::Error::other)?;
    run_with_terminal_cleanup(|stdout| {
        run_loop_with_refresh(
            stdout,
            mailbox_states,
            configs,
            RootWindowConfig::default(),
            dialog_default_bindings(),
            generic_default_bindings(),
        )
    })
}

/// Run the inbox application with sample data.
pub async fn run() -> Result<()> {
    run_with_options(
        RootWindowConfig::default(),
        dialog_default_bindings(),
        generic_default_bindings(),
    )
    .await
}

/// Run the inbox application with sample data and custom UI settings.
pub async fn run_with_options(
    config: RootWindowConfig,
    dialog_bindings: Bindings,
    global_bindings: Bindings,
) -> Result<()> {
    run_with_mailboxes_and_options(
        sample_mailbox_data(),
        config,
        dialog_bindings,
        global_bindings,
    )
    .await
}

/// Run the inbox application with provided data and custom UI settings.
pub async fn run_with_mailboxes_and_options(
    mailboxes: Vec<Mailbox>,
    config: RootWindowConfig,
    dialog_bindings: Bindings,
    global_bindings: Bindings,
) -> Result<()> {
    run_with_terminal_cleanup(|stdout| {
        run_loop(stdout, mailboxes, config, dialog_bindings, global_bindings)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox::state::InboxState;
    use agent_inbox_protocol::Body;
    use agent_inbox_protocol::From;
    use agent_inbox_protocol::MailboxName;

    fn make_test_mailbox(num_messages: usize) -> Mailbox {
        let messages: Vec<Message> = (0..num_messages)
            .map(|i| Message {
                msg_id: MessageID::default(),
                date: Utc::now(),
                from: From::new(format!("user{}@example.com", i)).expect("valid from"),
                body: Body::new(format!("Message {}", i)).expect("valid body"),
                wrap: false,
                tools: vec![],
            })
            .collect();
        Mailbox {
            name: MailboxName::new("Test").expect("valid name"),
            messages,
        }
    }

    #[test]
    fn scrolloff_stays_at_top_when_selection_near_start() {
        // With 10 messages and viewport of 5, SCROLLOFF=3.
        // ideal_max_pos = 5 - 1 - 3 = 1, meaning cursor can be at row 0 or 1 without scrolling.
        // Selecting message 0 or 1 keeps scroll at 0.
        // Selecting message 2 would put cursor at row 2, which > ideal_max_pos=1, so scroll=1.
        let mailbox = make_test_mailbox(10);
        let mut state = InboxState::new(vec![mailbox]);

        state.update_scroll_offset(5);
        assert_eq!(state.current_mailbox().unwrap().scroll_offset, 0);
        println!(
            "selected=0, scroll_offset={}",
            state.current_mailbox().unwrap().scroll_offset
        );

        state.current_mailbox_mut().unwrap().selected_message = 1;
        state.update_scroll_offset(5);
        assert_eq!(state.current_mailbox().unwrap().scroll_offset, 0);
        println!(
            "selected=1, scroll_offset={}",
            state.current_mailbox().unwrap().scroll_offset
        );

        // Message 2 causes scroll because we need 3 lines below cursor.
        // However, with viewport 5, scrolloff is clamped to 2.
        // ideal_max = 5 - 1 - 2 = 2. Pos 2 <= 2. No scroll needed.
        state.current_mailbox_mut().unwrap().selected_message = 2;
        state.update_scroll_offset(5);
        assert_eq!(state.current_mailbox().unwrap().scroll_offset, 0);
        println!(
            "selected=2, scroll_offset={}",
            state.current_mailbox().unwrap().scroll_offset
        );
    }

    #[test]
    fn scrolloff_scrolls_when_cursor_approaches_bottom() {
        // With 10 messages and viewport of 5, SCROLLOFF=3 means bottom_threshold = 5-3-1 = 1.
        // Selecting message 3 with scroll_offset=0 means pos_in_viewport=3 > 1, so scroll should
        // increase.
        let mailbox = make_test_mailbox(10);
        let mut state = InboxState::new(vec![mailbox]);

        state.current_mailbox_mut().unwrap().selected_message = 4;
        state.update_scroll_offset(5);
        // pos_in_viewport = 4 - 0 = 4, bottom_threshold = 5 - 3 - 1 = 1
        // overshoot = 4 - 1 = 3, so scroll = 0 + 3 = 3
        // But then we need selected=4 to be at pos 1 in viewport: 4 - 3 = 1. Good.
        println!(
            "selected=4, scroll_offset={}",
            state.current_mailbox().unwrap().scroll_offset
        );
        assert!(state.current_mailbox().unwrap().scroll_offset > 0);
    }

    #[test]
    fn scrolloff_does_not_scroll_past_end() {
        // With 10 messages and viewport of 5, max_scroll = 10 - 5 = 5.
        let mailbox = make_test_mailbox(10);
        let mut state = InboxState::new(vec![mailbox]);

        state.current_mailbox_mut().unwrap().selected_message = 9;
        state.update_scroll_offset(5);
        // max_scroll = 10 - 5 = 5
        println!(
            "selected=9, scroll_offset={}",
            state.current_mailbox().unwrap().scroll_offset
        );
        assert!(state.current_mailbox().unwrap().scroll_offset <= 5);
    }

    #[test]
    fn scrolloff_handles_small_list() {
        // With 3 messages and viewport of 10, no scrolling needed.
        let mailbox = make_test_mailbox(3);
        let mut state = InboxState::new(vec![mailbox]);

        state.current_mailbox_mut().unwrap().selected_message = 2;
        state.update_scroll_offset(10);
        assert_eq!(state.current_mailbox().unwrap().scroll_offset, 0);
        println!(
            "selected=2, scroll_offset={}",
            state.current_mailbox().unwrap().scroll_offset
        );
    }

    #[test]
    fn scrolloff_handles_empty_mailbox() {
        let mailbox = make_test_mailbox(0);
        let mut state = InboxState::new(vec![mailbox]);

        state.update_scroll_offset(5);
        assert_eq!(state.current_mailbox().unwrap().scroll_offset, 0);
    }

    #[test]
    fn scrolloff_scrolls_up_when_moving_back() {
        // Simulate scrolling down then back up.
        let mailbox = make_test_mailbox(20);
        let mut state = InboxState::new(vec![mailbox]);

        // Scroll down to message 10.
        state.current_mailbox_mut().unwrap().selected_message = 10;
        state.update_scroll_offset(5);
        let scroll_at_10 = state.current_mailbox().unwrap().scroll_offset;
        println!("selected=10, scroll_offset={}", scroll_at_10);

        // Now move back to message 5.
        state.current_mailbox_mut().unwrap().selected_message = 5;
        state.update_scroll_offset(5);
        let scroll_at_5 = state.current_mailbox().unwrap().scroll_offset;
        println!("selected=5, scroll_offset={}", scroll_at_5);

        // Scroll should have decreased.
        assert!(scroll_at_5 < scroll_at_10);
    }

    #[test]
    fn scrolloff_scrolls_up_when_moving_up() {
        // With 10 messages and viewport of 5, SCROLLOFF=3.
        // Selecting message 4 with scroll 4 (msg 4 at top).
        // Viewport 5 => effective scrolloff = min(3, (5-1)/2) = 2.
        // Top check: pos 0 < 2. Scroll = 4 - 2 = 2.
        let mailbox = make_test_mailbox(10);
        let mut state = InboxState::new(vec![mailbox]);

        // Setup: scroll down so we can scroll up.
        state.current_mailbox_mut().unwrap().scroll_offset = 4;
        state.current_mailbox_mut().unwrap().selected_message = 4;

        state.update_scroll_offset(5);

        println!("scroll: {}", state.current_mailbox().unwrap().scroll_offset);
        assert_eq!(state.current_mailbox().unwrap().scroll_offset, 2);
    }
}
