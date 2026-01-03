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

use std::cell::Cell;
use std::cell::RefCell;
use std::io::stdout;
use std::io::Result;
use std::io::Stdout;
use std::io::Write;
use std::panic;
use std::rc::Rc;

use agent_inbox_protocol::Client;
use agent_inbox_protocol::Mailbox;
use agent_inbox_protocol::MailboxProvider;
use agent_inbox_protocol::Message;
use agent_inbox_protocol::QueryParameters;
use chrono::Utc;
use crossterm::cursor::MoveTo;
use crossterm::event::read;
use crossterm::event::Event;
use crossterm::style::Attribute;
use crossterm::style::Color;
use crossterm::style::Print;
use crossterm::ExecutableCommand;

use crate::default_global_functions;
use crate::dialog_default_bindings;
use crate::generic_default_bindings;
use crate::global_function_dispatcher_active;
use crate::lookup_binding;
use crate::mutt_curses_set_color;
use crate::mutt_curses_set_color_by_id;
use crate::mutt_curses_set_normal_backed_color_by_id;
use crate::mutt_resize_screen;
use crate::window_reflow;
use crate::AttrColor;
use crate::ColorId;
use crate::FunctionRetval;
use crate::GlobalFunctionEntry;
use crate::GuiContext;
use crate::HelpData;
use crate::HelpItem;
use crate::IndexPagerLayout;
use crate::MuttWindow;
use crate::OpCode;
use crate::RootWindow;
use crate::WindowActionFlags;
use crate::WindowType;

/// Dirty flags for selective redrawing.
#[derive(Default)]
struct DirtyFlags {
    help_bar: Cell<bool>,
    sidebar: Cell<bool>,
    index: Cell<bool>,
    index_bar: Cell<bool>,
    pager: Cell<bool>,
    pager_bar: Cell<bool>,
    message: Cell<bool>,
}

impl DirtyFlags {
    fn new() -> Self {
        Self {
            help_bar: Cell::new(true),
            sidebar: Cell::new(true),
            index: Cell::new(true),
            index_bar: Cell::new(true),
            pager: Cell::new(true),
            pager_bar: Cell::new(true),
            message: Cell::new(true),
        }
    }

    fn mark_all(&self) {
        self.help_bar.set(true);
        self.sidebar.set(true);
        self.index.set(true);
        self.index_bar.set(true);
        self.pager.set(true);
        self.pager_bar.set(true);
        self.message.set(true);
    }

    fn mark_message_views(&self) {
        self.index.set(true);
        self.index_bar.set(true);
        self.pager.set(true);
        self.pager_bar.set(true);
        self.message.set(true);
    }

    fn mark_mailbox_change(&self) {
        self.sidebar.set(true);
        self.mark_message_views();
    }
}

/// Internal mailbox state used by the inbox.
#[derive(Clone)]
struct MailboxState {
    mailbox: Mailbox,
    selected_message: usize,
}

impl MailboxState {
    fn new(mailbox: Mailbox) -> Self {
        Self {
            mailbox,
            selected_message: 0,
        }
    }
}

/// Render data extracted from InboxState for drawing.
#[derive(Clone)]
struct RenderData {
    selected_mailbox: usize,
    selected_message: usize,
    mailbox_names: Vec<String>,
    messages: Vec<Message>,
    status_message: String,
}

/// Inbox state.
struct InboxState {
    selected_mailbox: usize,
    mailbox_states: Vec<MailboxState>,
    status_message: String,
    dirty: DirtyFlags,
}

impl InboxState {
    /// Extracts render data for drawing.
    fn render_data(&self) -> RenderData {
        let mailbox_state = &self.mailbox_states[self.selected_mailbox];
        RenderData {
            selected_mailbox: self.selected_mailbox,
            selected_message: mailbox_state.selected_message,
            mailbox_names: self
                .mailbox_states
                .iter()
                .map(|m| m.mailbox.name.as_str().to_string())
                .collect(),
            messages: mailbox_state.mailbox.messages.clone(),
            status_message: self.status_message.clone(),
        }
    }

    fn current_mailbox(&self) -> &MailboxState {
        &self.mailbox_states[self.selected_mailbox]
    }

    fn current_mailbox_mut(&mut self) -> &mut MailboxState {
        &mut self.mailbox_states[self.selected_mailbox]
    }

    fn new(mailboxes: Vec<Mailbox>) -> Self {
        let mailbox_states: Vec<MailboxState> =
            mailboxes.into_iter().map(MailboxState::new).collect();
        Self {
            selected_mailbox: 0,
            mailbox_states,
            status_message:
                "Welcome! Press 'q' to quit, j/k to navigate messages, J/K for mailboxes."
                    .to_string(),
            dirty: DirtyFlags::new(),
        }
    }

    fn select_next_message(&mut self) {
        let mailbox = self.current_mailbox_mut();
        if !mailbox.mailbox.messages.is_empty()
            && mailbox.selected_message < mailbox.mailbox.messages.len() - 1
        {
            mailbox.selected_message += 1;
            self.status_message = format!(
                "Selected message {}",
                self.current_mailbox().selected_message + 1
            );
            self.dirty.mark_message_views();
        }
    }

    fn select_prev_message(&mut self) {
        let mailbox = self.current_mailbox_mut();
        if mailbox.selected_message > 0 {
            mailbox.selected_message -= 1;
            self.status_message = format!(
                "Selected message {}",
                self.current_mailbox().selected_message + 1
            );
            self.dirty.mark_message_views();
        }
    }

    fn select_next_mailbox(&mut self) {
        if self.mailbox_states.len() > 1 && self.selected_mailbox < self.mailbox_states.len() - 1 {
            self.selected_mailbox += 1;
            self.status_message = format!(
                "Selected mailbox: {}",
                self.current_mailbox().mailbox.name.as_str()
            );
            self.dirty.mark_mailbox_change();
        }
    }

    fn select_prev_mailbox(&mut self) {
        if self.selected_mailbox > 0 {
            self.selected_mailbox -= 1;
            self.status_message = format!(
                "Selected mailbox: {}",
                self.current_mailbox().mailbox.name.as_str()
            );
            self.dirty.mark_mailbox_change();
        }
    }
}

/// Creates the default sample mailbox data.
pub fn sample_mailbox_data() -> Vec<Mailbox> {
    use agent_inbox_protocol::Body;
    use agent_inbox_protocol::From;
    use agent_inbox_protocol::MailboxName;

    let inbox = Mailbox {
        name: MailboxName::new("INBOX").expect("valid mailbox name"),
        messages: vec![
            Message {
                date: Utc::now(),
                from: From::new("alice@example.com").expect("valid from"),
                body: Body::new("Hello World").expect("valid body"),
                wrap: false,
            },
            Message {
                date: Utc::now(),
                from: From::new("bob@example.com").expect("valid from"),
                body: Body::new("Re: Hello World").expect("valid body"),
                wrap: false,
            },
            Message {
                date: Utc::now(),
                from: From::new("charlie@example.com").expect("valid from"),
                body: Body::new("Project Update").expect("valid body"),
                wrap: false,
            },
        ],
    };

    let sent = Mailbox {
        name: MailboxName::new("Sent").expect("valid mailbox name"),
        messages: vec![
            Message {
                date: Utc::now(),
                from: From::new("me@example.com").expect("valid from"),
                body: Body::new("Re: Hello World").expect("valid body"),
                wrap: false,
            },
            Message {
                date: Utc::now(),
                from: From::new("me@example.com").expect("valid from"),
                body: Body::new("Meeting Tomorrow").expect("valid body"),
                wrap: false,
            },
        ],
    };

    let drafts = Mailbox {
        name: MailboxName::new("Drafts").expect("valid mailbox name"),
        messages: vec![Message {
            date: Utc::now(),
            from: From::new("me@example.com").expect("valid from"),
            body: Body::new("Draft: Proposal").expect("valid body"),
            wrap: false,
        }],
    };

    let trash = Mailbox {
        name: MailboxName::new("Trash").expect("valid mailbox name"),
        messages: vec![Message {
            date: Utc::now(),
            from: From::new("spam@example.com").expect("valid from"),
            body: Body::new("You've won!").expect("valid body"),
            wrap: false,
        }],
    };

    vec![inbox, sent, drafts, trash]
}

/// Fetches mailboxes from an agent-inbox-protocol server.
///
/// This function connects to the specified base URL (e.g., `http://localhost:8080/inbox`)
/// and queries for all mailboxes using the provided query parameters.
pub fn fetch_mailboxes_blocking(
    base_url: &str,
    query: QueryParameters,
) -> std::result::Result<Vec<Mailbox>, String> {
    let mut client = Client::new(base_url);
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| format!("failed to create tokio runtime: {}", e))?;

    runtime.block_on(async {
        let result = client
            .query(query)
            .await
            .map_err(|e| format!("HTTP query failed: {}", e))?;
        Ok(result.into_mailboxes())
    })
}

fn with_inbox_state(win: &mut MuttWindow, f: impl FnOnce(&mut InboxState)) -> FunctionRetval {
    if let Some(state) = win.wdata_mut::<InboxState>() {
        f(state);
        FunctionRetval::Done
    } else {
        FunctionRetval::Unhandled
    }
}

/// Global function handler for NextEntry (j/Down).
fn op_next_entry(win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    with_inbox_state(win, |state| state.select_next_message())
}

/// Global function handler for PrevEntry (k/Up).
fn op_prev_entry(win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    with_inbox_state(win, |state| state.select_prev_message())
}

/// Global function handler for SidebarNext (J).
fn op_sidebar_next(win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    with_inbox_state(win, |state| state.select_next_mailbox())
}

/// Global function handler for SidebarPrev (K).
fn op_sidebar_prev(win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    with_inbox_state(win, |state| state.select_prev_mailbox())
}

/// Inbox-specific global functions for navigation.
fn inbox_global_functions() -> Vec<GlobalFunctionEntry> {
    let mut funcs: Vec<GlobalFunctionEntry> = default_global_functions().to_vec();
    funcs.extend([
        GlobalFunctionEntry {
            op: OpCode::NextEntry,
            function: op_next_entry,
        },
        GlobalFunctionEntry {
            op: OpCode::PrevEntry,
            function: op_prev_entry,
        },
        GlobalFunctionEntry {
            op: OpCode::SidebarNext,
            function: op_sidebar_next,
        },
        GlobalFunctionEntry {
            op: OpCode::SidebarPrev,
            function: op_sidebar_prev,
        },
    ]);
    funcs
}

fn move_to_row(out: &mut dyn Write, win: &MuttWindow, row: u16) -> Result<()> {
    out.execute(MoveTo(
        win.state.col_offset as u16,
        win.state.row_offset as u16 + row,
    ))?;
    Ok(())
}

fn write_padded(out: &mut dyn Write, win: &MuttWindow, text: &str) -> Result<()> {
    let cols = win.state.cols as usize;
    let truncated: String = text.chars().take(cols).collect();
    let padding = cols.saturating_sub(truncated.chars().count());
    out.execute(Print(&truncated))?;
    if padding > 0 {
        out.execute(Print(" ".repeat(padding)))?;
    }
    Ok(())
}

/// Draws the sidebar.
fn draw_sidebar(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    win: &MuttWindow,
    data: &RenderData,
) -> Result<()> {
    for row in 0u16..(win.state.rows.try_into().expect("positive i16 to fit u16")) {
        move_to_row(out, win, row)?;

        if (row as usize) < data.mailbox_names.len() {
            let mailbox = &data.mailbox_names[row as usize];
            let is_selected = row as usize == data.selected_mailbox;

            if is_selected {
                let selected = AttrColor::new(
                    Some(Color::White),
                    Some(Color::DarkGrey),
                    &[Attribute::Bold],
                );
                mutt_curses_set_color(ctx, out, &selected)?;
            } else {
                mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
            }

            let display = format!(" {} ", mailbox);
            write_padded(out, win, &display)?;
            mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
        } else {
            mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
            write_padded(out, win, "")?;
        }
    }
    Ok(())
}

/// Draws the message index.
fn draw_index(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    win: &MuttWindow,
    data: &RenderData,
) -> Result<()> {
    for row in 0u16..(win.state.rows.try_into().expect("positive i16 to fit u16")) {
        move_to_row(out, win, row)?;

        if (row as usize) < data.messages.len() {
            let msg = &data.messages[row as usize];
            let is_selected = row as usize == data.selected_message;

            if is_selected {
                let selected = AttrColor::new(
                    Some(Color::White),
                    Some(Color::DarkCyan),
                    &[Attribute::Bold],
                );
                mutt_curses_set_color(ctx, out, &selected)?;
            } else {
                mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
            }

            let date_str = msg.date.format("%Y-%m-%d").to_string();
            let line = format!(
                "{} | {:20} | {}",
                date_str,
                msg.from.as_str(),
                msg.body.as_str()
            );
            write_padded(out, win, &line)?;
            mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
        } else {
            mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
            write_padded(out, win, "")?;
        }
    }
    Ok(())
}

/// Draws the pager (message view).
fn draw_pager(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    win: &MuttWindow,
    data: &RenderData,
) -> Result<()> {
    mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;

    if data.messages.is_empty() {
        for row in 0u16..(win.state.rows.try_into().expect("positive i16 to fit u16")) {
            move_to_row(out, win, row)?;
            write_padded(out, win, "")?;
        }
        return Ok(());
    }

    let message = &data.messages[data.selected_message];
    let lines = [
        format!("From: {}", message.from.as_str()),
        format!("Date: {}", message.date.format("%Y-%m-%d %H:%M:%S")),
        String::new(),
        message.body.as_str().to_string(),
    ];

    for row in 0u16..(win.state.rows.try_into().expect("positive i16 to fit u16")) {
        move_to_row(out, win, row)?;

        if (row as usize) < lines.len() {
            let line = &lines[row as usize];
            write_padded(out, win, line)?;
        } else {
            write_padded(out, win, "")?;
        }
    }
    Ok(())
}

/// Draws a status bar.
fn draw_status_bar(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    win: &MuttWindow,
    title: &str,
) -> Result<()> {
    move_to_row(out, win, 0)?;
    mutt_curses_set_normal_backed_color_by_id(ctx, out, ColorId::Status)?;

    write_padded(out, win, title)?;
    mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
    Ok(())
}

/// Draws the message window.
fn draw_message_window(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    win: &MuttWindow,
    msg: &str,
) -> Result<()> {
    move_to_row(out, win, 0)?;
    mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;

    write_padded(out, win, msg)?;
    Ok(())
}

/// Window handles.
struct Windows {
    #[allow(dead_code)]
    root: RootWindow,
    ctx: GuiContext,
    layout: IndexPagerLayout,
    help_bar: Rc<RefCell<MuttWindow>>,
    message_container: Rc<RefCell<MuttWindow>>,
    global_functions: Vec<GlobalFunctionEntry>,
}

impl Windows {
    fn new(stdout: &mut Stdout, mailboxes: Vec<Mailbox>) -> Result<Self> {
        let mut ctx = GuiContext::new();
        let root = RootWindow::new(stdout)?;
        ctx.register_root_window(root.root());
        let help_data = Rc::new(HelpData::from_items(vec![
            HelpItem::new("q", "Quit"),
            HelpItem::new("j/k", "Navigate"),
            HelpItem::new("J/K", "Mailbox"),
            HelpItem::new("?", "Help"),
        ]));

        let layout = IndexPagerLayout::new(WindowType::DlgIndex, Some(help_data));
        {
            let mut dlg_win = layout.dialog.borrow_mut();
            dlg_win.wdata = Some(Box::new(InboxState::new(mailboxes)));
        }

        let global_functions = inbox_global_functions();

        MuttWindow::add_child(root.all_dialogs(), Rc::clone(&layout.dialog));

        // Initial reflow
        window_reflow(root.root());

        let help_bar = Rc::clone(root.help_bar());
        let message_container = Rc::clone(root.message_container());

        Ok(Self {
            root,
            ctx,
            layout,
            help_bar,
            message_container,
            global_functions,
        })
    }

    fn handle_resize(&mut self) -> Result<()> {
        mutt_resize_screen(&mut self.ctx, &mut self.root)
    }

    /// Returns a reference to the inbox state stored in the dialog.
    fn state(&self) -> std::cell::Ref<'_, InboxState> {
        std::cell::Ref::map(self.layout.dialog.borrow(), |dlg| {
            dlg.wdata_ref::<InboxState>().expect("InboxState missing")
        })
    }

    /// Returns a mutable reference to the inbox state stored in the dialog.
    fn state_mut(&self) -> std::cell::RefMut<'_, InboxState> {
        std::cell::RefMut::map(self.layout.dialog.borrow_mut(), |dlg| {
            dlg.wdata_mut::<InboxState>().expect("InboxState missing")
        })
    }
}

/// Redraw only dirty windows.
fn redraw(stdout: &mut Stdout, windows: &mut Windows) -> Result<()> {
    // Extract render data once to avoid borrow conflicts with windows.ctx.
    let render_data = windows.state().render_data();

    if windows.state().dirty.help_bar.get() {
        windows.help_bar.borrow_mut().actions |=
            WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
        MuttWindow::redraw(&windows.help_bar, &mut windows.ctx, stdout)?;
        windows.state().dirty.help_bar.set(false);
    }

    if windows.state().dirty.sidebar.get() {
        let win = windows.layout.sidebar.borrow();
        draw_sidebar(&mut windows.ctx, stdout, &win, &render_data)?;
        windows.state().dirty.sidebar.set(false);
    }

    if windows.state().dirty.index.get() {
        let win = windows.layout.index_window.borrow();
        draw_index(&mut windows.ctx, stdout, &win, &render_data)?;
        windows.state().dirty.index.set(false);
    }

    if windows.state().dirty.index_bar.get() {
        let mailbox_name = &render_data.mailbox_names[render_data.selected_mailbox];
        let msg_count = render_data.messages.len();
        let title = if msg_count > 0 {
            format!(
                " {} [{}/{}] ",
                mailbox_name,
                render_data.selected_message + 1,
                msg_count
            )
        } else {
            format!(" {} [empty] ", mailbox_name)
        };
        let win = windows.layout.index_bar.borrow();
        draw_status_bar(&mut windows.ctx, stdout, &win, &title)?;
        windows.state().dirty.index_bar.set(false);
    }

    if windows.state().dirty.pager.get() {
        let win = windows.layout.pager_window.borrow();
        draw_pager(&mut windows.ctx, stdout, &win, &render_data)?;
        windows.state().dirty.pager.set(false);
    }

    if windows.state().dirty.pager_bar.get() {
        let title = if !render_data.messages.is_empty() {
            format!(
                " Message: {} ",
                render_data.messages[render_data.selected_message]
                    .body
                    .as_str()
            )
        } else {
            " No messages ".to_string()
        };
        let win = windows.layout.pager_bar.borrow();
        draw_status_bar(&mut windows.ctx, stdout, &win, &title)?;
        windows.state().dirty.pager_bar.set(false);
    }

    if windows.state().dirty.message.get() {
        let win = windows.message_container.borrow();
        draw_message_window(&mut windows.ctx, stdout, &win, &render_data.status_message)?;
        windows.state().dirty.message.set(false);
    }

    if let Some(top) = windows.root.all_dialogs().borrow().children.last() {
        if top.borrow().window_type == WindowType::DlgHelp {
            MuttWindow::redraw(top, &mut windows.ctx, stdout)?;
        }
    }

    stdout.flush()?;
    Ok(())
}

fn run_loop(stdout: &mut Stdout, mailboxes: Vec<Mailbox>) -> Result<()> {
    let mut windows = Windows::new(stdout, mailboxes)?;

    // Initial draw
    redraw(stdout, &mut windows)?;

    // Event loop - blocks until an event occurs (no polling, no flickering)
    loop {
        let help_before = windows
            .root
            .all_dialogs()
            .borrow()
            .children
            .last()
            .map(|top| top.borrow().window_type == WindowType::DlgHelp)
            .unwrap_or(false);

        match read()? {
            Event::Key(key) => {
                let op = lookup_binding(dialog_default_bindings(), key)
                    .or_else(|| lookup_binding(generic_default_bindings(), key));
                if let Some(op) = op {
                    let ret = global_function_dispatcher_active(
                        &windows.layout.dialog,
                        &mut windows.ctx,
                        op,
                        &windows.global_functions,
                    );
                    if ret == FunctionRetval::Abort {
                        break;
                    }
                    if ret != FunctionRetval::Unhandled {
                        windows.state().dirty.help_bar.set(true);
                    }
                }
            }
            Event::Resize(cols, rows) => {
                windows.handle_resize()?;
                let mut state = windows.state_mut();
                state.status_message = format!("Resized to {}x{}", cols, rows);
                state.dirty.mark_all();
            }
            Event::Mouse(_) => {}
            Event::FocusGained => {}
            Event::FocusLost => {}
            Event::Paste(_) => {}
        }

        let help_after = windows
            .root
            .all_dialogs()
            .borrow()
            .children
            .last()
            .map(|top| top.borrow().window_type == WindowType::DlgHelp)
            .unwrap_or(false);
        if help_before != help_after {
            windows.state().dirty.mark_all();
        }

        // Redraw only what changed
        redraw(stdout, &mut windows)?;
    }

    Ok(())
}

/// Run the inbox application with mailboxes from an agent-inbox-protocol server.
pub fn run_with_mailboxes(mailboxes: Vec<Mailbox>) -> Result<()> {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let mut out = stdout();
        let _ = RootWindow::cleanup(&mut out);
        default_hook(info);
    }));

    let mut stdout = stdout();
    let result = run_loop(&mut stdout, mailboxes);
    let _ = RootWindow::cleanup(&mut stdout);
    result?;

    Ok(())
}

/// Run the inbox application by fetching from an agent-inbox-protocol server.
pub fn run_from_server(base_url: &str) -> Result<()> {
    let mailboxes = fetch_mailboxes_blocking(base_url, QueryParameters::default())
        .map_err(std::io::Error::other)?;
    run_with_mailboxes(mailboxes)
}

/// Run the inbox application with sample data.
pub fn run() -> Result<()> {
    run_with_mailboxes(sample_mailbox_data())
}
