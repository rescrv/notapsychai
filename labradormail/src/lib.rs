//! A Rust recreation of neomutt/gui using crossterm.
//!
//! This crate provides a hierarchical window management system with automatic layout (reflow),
//! notification-based updates, focus management, and dialog stacking.

mod action;
mod agent_dialog;
mod command;
mod context;
mod curs_lib;
mod geom;
mod global;
mod help_bar;
mod help_data;
mod help_dialog;
mod inbox;
mod message_window;
mod reflow;
mod render;
mod root_window;
mod sbar;
mod ui;

mod window;

pub use action::Action;
pub use global::Bindings;
pub use global::Key;
pub use render::Renderer;
pub use root_window::RootWindowConfig;
pub use ui::Ui;

pub mod prelude {
    pub use crate::action::Action;
    pub use crate::agent_dialog::agent_dialog_handle_key;
    pub use crate::agent_dialog::agent_dialog_poll;
    pub use crate::agent_dialog::agent_dialog_scroll;
    pub use crate::agent_dialog::AgentDialog;
    pub use crate::agent_dialog::AgentDialogContentData;
    pub use crate::context::AttrColor;
    pub use crate::context::ColorId;
    pub use crate::context::GuiContext;
    pub use crate::curs_lib::char_width;
    pub use crate::curs_lib::expand_tabs;
    pub use crate::curs_lib::mw_enter_fname;
    pub use crate::curs_lib::truncate_str_width;
    pub use crate::curs_lib::FileCompletionData;
    pub use crate::curs_lib::SelectFileFlags;
    pub use crate::curs_lib::Truncation;
    pub use crate::geom::Point;
    pub use crate::geom::Rect;
    pub use crate::geom::Size;
    pub use crate::global::default_global_functions;
    pub use crate::global::dialog_default_bindings;
    pub use crate::global::generic_default_bindings;
    pub use crate::global::global_function_dispatcher;
    pub use crate::global::global_function_dispatcher_active;
    pub use crate::global::global_function_dispatcher_handle_help;
    pub use crate::global::help_data_from_bindings;
    pub use crate::global::lookup_binding;
    pub use crate::global::Bindings;
    pub use crate::global::FunctionRetval;
    pub use crate::global::GlobalFunction;
    pub use crate::global::GlobalFunctionEntry;
    pub use crate::global::Key;
    pub use crate::help_bar::collect_bindings_from_focus;
    pub use crate::help_bar::HelpBar;
    pub use crate::help_bar::HelpBarWindowData;
    pub use crate::help_data::HelpData;
    pub use crate::help_data::HelpItem;
    pub use crate::help_dialog::help_dialog_scroll;
    pub use crate::help_dialog::HelpDialog;
    pub use crate::inbox::fetch_mailboxes;
    pub use crate::inbox::run;
    pub use crate::inbox::run_from_servers;
    pub use crate::inbox::run_with_mailboxes;
    pub use crate::inbox::sample_mailbox_data;
    pub use crate::inbox::ServerConfig;
    pub use crate::inbox::ToolOrigin;
    pub use crate::message_window::MessageWindow;
    pub use crate::message_window::MsgWinWindowData;
    pub use crate::message_window::MwChar;
    pub use crate::message_window::MwChunk;
    pub use crate::reflow::window_reflow;
    pub use crate::render::Renderer;
    pub use crate::root_window::resize_screen;
    pub use crate::root_window::RootWindow;
    pub use crate::root_window::RootWindowConfig;
    pub use crate::sbar::SBarPrivateData;
    pub use crate::sbar::StatusBar;
    pub use crate::ui::Ui;
    pub use crate::window::window_status_on_top;
    pub use crate::window::CursorBehavior;
    pub use crate::window::CursorState;
    pub use crate::window::EventWindow;
    pub use crate::window::NotifyWindow;
    pub use crate::window::RenderMode;
    pub use crate::window::Window;
    pub use crate::window::WindowActionFlags;
    pub use crate::window::WindowId;
    pub use crate::window::WindowNotifyFlags;
    pub use crate::window::WindowObserver;
    pub use crate::window::WindowObserverFn;
    pub use crate::window::WindowOrientation;
    pub use crate::window::WindowSize;
    pub use crate::window::WindowState;
    pub use crate::window::WindowTree;
    pub use crate::window::WindowType;
}

// Re-export agent-inbox-protocol types for convenience.
pub use agent_inbox_protocol::router;
pub use agent_inbox_protocol::Body;
pub use agent_inbox_protocol::Client;
pub use agent_inbox_protocol::From;
pub use agent_inbox_protocol::Mailbox;
pub use agent_inbox_protocol::MailboxName;
pub use agent_inbox_protocol::MailboxProvider;
pub use agent_inbox_protocol::Message;
pub use agent_inbox_protocol::QueryParameters;
pub use agent_inbox_protocol::QueryResult;

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    #[test]
    fn create_window() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        assert_eq!(tree.get(win).req_size.cols, 80);
        assert_eq!(tree.get(win).req_size.rows, 24);
    }
}
