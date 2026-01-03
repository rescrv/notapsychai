#![deny(missing_docs)]
//! A Rust recreation of neomutt/gui using crossterm.
//!
//! This crate provides a hierarchical window management system with automatic layout (reflow),
//! notification-based updates, focus management, and dialog stacking.

mod context;
mod command;
mod curs_lib;
mod dialog;
mod global;
mod help_bar;
mod help_data;
mod help_dialog;
mod inbox;
mod layout;
mod message_window;
mod msgcont;
mod mutt_curses;
mod opcodes;
mod reflow;
mod root_window;
mod sbar;
mod simple;
mod terminal;
mod window;

pub use context::GuiContext;
pub use curs_lib::mutt_addwch;
pub use curs_lib::mutt_any_key_to_continue;
pub use curs_lib::mutt_beep;
pub use curs_lib::mutt_char_width;
pub use curs_lib::mutt_edit_file;
pub use curs_lib::mutt_endwin;
pub use curs_lib::mutt_need_hard_redraw;
pub use curs_lib::mutt_paddstr;
pub use curs_lib::mutt_query_exit;
pub use curs_lib::mutt_refresh;
pub use curs_lib::mutt_str_expand_tabs;
pub use curs_lib::mutt_strnwidth;
pub use curs_lib::mutt_strwidth;
pub use curs_lib::mutt_wstr_trunc;
pub use curs_lib::mw_enter_fname;
pub use curs_lib::mw_what_key;
pub use curs_lib::FileCompletionData;
pub use curs_lib::SelectFileFlags;
pub use dialog::AllDialogsWindow;
pub use dialog::Dialog;
pub use global::default_global_functions;
pub use global::dialog_default_bindings;
pub use global::dialog_menu_functions;
pub use global::dialog_stub_functions;
pub use global::generic_default_bindings;
pub use global::generic_menu_functions;
pub use global::generic_stub_functions;
pub use global::global_function_dispatcher;
pub use global::global_function_dispatcher_active;
pub use global::global_function_dispatcher_rc;
pub use global::help_data_from_bindings;
pub use global::lookup_binding;
pub use global::FunctionRetval;
pub use global::GlobalFunction;
pub use global::GlobalFunctionEntry;
pub use global::Key;
pub use global::KeyBinding;
pub use global::MenuFuncFlags;
pub use global::MenuFuncOp;
pub use help_bar::HelpBar;
pub use help_bar::HelpBarWindowData;
pub use help_data::HelpData;
pub use help_data::HelpItem;
pub use help_dialog::HelpDialog;
pub use inbox::fetch_mailboxes_blocking;
pub use inbox::run;
pub use inbox::run_from_server;
pub use inbox::run_with_mailboxes;
pub use inbox::sample_mailbox_data;

// Re-export agent-inbox-protocol types for convenience.
pub use agent_inbox_protocol::Body;
pub use agent_inbox_protocol::Client;
pub use agent_inbox_protocol::From;
pub use agent_inbox_protocol::Mailbox;
pub use agent_inbox_protocol::MailboxName;
pub use agent_inbox_protocol::MailboxProvider;
pub use agent_inbox_protocol::Message;
pub use agent_inbox_protocol::QueryParameters;
pub use agent_inbox_protocol::QueryResult;
pub use layout::IndexPagerLayout;
pub use layout::IndexPagerLayoutConfig;
pub use message_window::MessageWindow;
pub use message_window::MsgWinWindowData;
pub use message_window::MwChar;
pub use message_window::MwChunk;
pub use msgcont::MessageContainer;
pub use mutt_curses::mutt_curses_set_color;
pub use mutt_curses::mutt_curses_set_color_by_id;
pub use mutt_curses::mutt_curses_set_cursor;
pub use mutt_curses::mutt_curses_set_normal_backed_color_by_id;
pub use mutt_curses::mutt_resize_screen;
pub use mutt_curses::simple_color_apply_config;
pub use mutt_curses::AttrColor;
pub use mutt_curses::ColorConfigEntry;
pub use mutt_curses::ColorId;
pub use opcodes::opcodes_get_description;
pub use opcodes::opcodes_get_name;
pub use opcodes::OpCode;
pub use reflow::window_reflow;
pub use root_window::RootWindow;
pub use root_window::RootWindowConfig;
pub use sbar::SBarPrivateData;
pub use sbar::StatusBar;
pub use simple::SimpleDialog;
pub use simple::SimpleDialogConfig;
pub use simple::SimpleDialogWindows;
pub use terminal::mutt_ts_capability;
pub use terminal::mutt_ts_icon;
pub use terminal::mutt_ts_status;
pub use terminal::ResizeHandler;
pub use terminal::Terminal;
pub use window::window_status_on_top;
pub use window::CursorState;
pub use window::EventWindow;
pub use window::MuttWindow;
pub use window::NotifyWindow;
pub use window::WindowActionFlags;
pub use window::WindowNotifyFlags;
pub use window::WindowObserver;
pub use window::WindowObserverFn;
pub use window::WindowOrientation;
pub use window::WindowSize;
pub use window::WindowState;
pub use window::WindowType;

/// Maximum number of rows for message window.
pub const MSGWIN_MAX_ROWS: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_window() {
        let win = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        assert_eq!(win.borrow().req_cols, 80);
        assert_eq!(win.borrow().req_rows, 24);
    }
}
