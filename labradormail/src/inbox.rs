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
use std::future::Future;
use std::io::stdout;
use std::io::Result;
use std::io::Stdout;
use std::io::Write;
use std::panic;
use std::pin::Pin;
use std::rc::Rc;

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

/// Threading metadata for rendering a tree.
#[derive(Clone, Default)]
pub struct ThreadInfo {
    /// Depth of the message in the thread tree.
    pub depth: usize,
    /// Whether this message is the last sibling at its level.
    pub is_last: bool,
    /// For each ancestor level, whether there are more siblings below.
    pub ancestors_have_more: Vec<bool>,
}

/// A single email message.
#[derive(Clone)]
pub struct Message {
    /// Date of the message.
    pub date: String,
    /// Sender of the message.
    pub from: String,
    /// Subject line.
    pub subject: String,
    /// Threading metadata.
    pub thread: ThreadInfo,
}

impl Message {
    /// Creates a new message with the given fields.
    pub fn new(date: &str, from: &str, subject: &str, thread: ThreadInfo) -> Self {
        Self {
            date: date.to_string(),
            from: from.to_string(),
            subject: subject.to_string(),
            thread,
        }
    }
}

/// Result type for mailbox operations.
pub type MailboxResult<T> = std::result::Result<T, MailboxError>;

/// Error type for mailbox operations.
#[derive(Debug, Clone)]
pub struct MailboxError {
    /// Human-readable description of the error.
    pub message: String,
}

impl std::fmt::Display for MailboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MailboxError {}

impl MailboxError {
    /// Creates a new mailbox error with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Async trait for mailbox providers.
///
/// Implementors of this trait provide access to email messages from various sources
/// (e.g., local storage, IMAP servers, etc.).
pub trait Mailbox: Send + Sync {
    /// Returns the display name of the mailbox.
    fn name(&self) -> &str;

    /// Fetches the list of messages in the mailbox.
    fn messages(&self) -> Pin<Box<dyn Future<Output = MailboxResult<Vec<Message>>> + Send + '_>>;

    /// Returns the total count of messages (may be cached).
    fn message_count(&self) -> Pin<Box<dyn Future<Output = MailboxResult<usize>> + Send + '_>>;
}

/// Internal mailbox state used by the inbox.
#[derive(Clone)]
struct MailboxState {
    name: String,
    messages: Vec<Message>,
    selected_message: usize,
}

impl MailboxState {
    fn new(name: &str, messages: Vec<Message>) -> Self {
        Self {
            name: name.to_string(),
            messages,
            selected_message: 0,
        }
    }
}

/// A simple in-memory mailbox implementation.
pub struct InMemoryMailbox {
    name: String,
    messages: Vec<Message>,
}

impl InMemoryMailbox {
    /// Creates a new in-memory mailbox with the given name and messages.
    pub fn new(name: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            name: name.into(),
            messages,
        }
    }
}

impl Mailbox for InMemoryMailbox {
    fn name(&self) -> &str {
        &self.name
    }

    fn messages(&self) -> Pin<Box<dyn Future<Output = MailboxResult<Vec<Message>>> + Send + '_>> {
        let messages = self.messages.clone();
        Box::pin(async move { Ok(messages) })
    }

    fn message_count(&self) -> Pin<Box<dyn Future<Output = MailboxResult<usize>> + Send + '_>> {
        let count = self.messages.len();
        Box::pin(async move { Ok(count) })
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
        let mailbox = &self.mailbox_states[self.selected_mailbox];
        RenderData {
            selected_mailbox: self.selected_mailbox,
            selected_message: mailbox.selected_message,
            mailbox_names: self.mailbox_states.iter().map(|m| m.name.clone()).collect(),
            messages: mailbox.messages.clone(),
            status_message: self.status_message.clone(),
        }
    }

    fn current_mailbox(&self) -> &MailboxState {
        &self.mailbox_states[self.selected_mailbox]
    }

    fn current_mailbox_mut(&mut self) -> &mut MailboxState {
        &mut self.mailbox_states[self.selected_mailbox]
    }

    fn new(mailboxes: Vec<Box<dyn Mailbox>>) -> Self {
        let mailbox_states = load_mailbox_states(&mailboxes);
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
        if mailbox.selected_message < mailbox.messages.len() - 1 {
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
            self.status_message = format!("Selected mailbox: {}", self.current_mailbox().name);
            self.dirty.mark_mailbox_change();
        }
    }

    fn select_prev_mailbox(&mut self) {
        if self.selected_mailbox > 0 {
            self.selected_mailbox -= 1;
            self.status_message = format!("Selected mailbox: {}", self.current_mailbox().name);
            self.dirty.mark_mailbox_change();
        }
    }
}

/// Synchronously loads mailbox states from the mailbox trait objects.
///
/// This uses a simple blocking executor since the inbox runs in a synchronous context.
fn load_mailbox_states(mailboxes: &[Box<dyn Mailbox>]) -> Vec<MailboxState> {
    mailboxes
        .iter()
        .map(|mb| {
            let name = mb.name().to_string();
            let messages = futures_block_on(mb.messages()).unwrap_or_default();
            MailboxState::new(&name, messages)
        })
        .collect()
}

/// Simple blocking executor for futures in synchronous context.
fn futures_block_on<T>(future: Pin<Box<dyn Future<Output = T> + Send + '_>>) -> T {
    use std::task::Context;
    use std::task::Poll;
    use std::task::RawWaker;
    use std::task::RawWakerVTable;
    use std::task::Waker;

    fn dummy_raw_waker() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            dummy_raw_waker()
        }
        let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);
        RawWaker::new(std::ptr::null(), vtable)
    }

    let waker = unsafe { Waker::from_raw(dummy_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut pinned = future;

    loop {
        match pinned.as_mut().poll(&mut cx) {
            Poll::Ready(result) => return result,
            Poll::Pending => {
                std::hint::spin_loop();
            }
        }
    }
}

/// Creates the default sample mailbox data.
pub fn sample_mailbox_data() -> Vec<Box<dyn Mailbox>> {
    let inbox = InMemoryMailbox::new(
        "INBOX",
        vec![
            Message::new(
                "2024-01-15",
                "alice@example.com",
                "Hello World",
                ThreadInfo {
                    depth: 0,
                    is_last: false,
                    ancestors_have_more: Vec::new(),
                },
            ),
            Message::new(
                "2024-01-14",
                "bob@example.com",
                "Re: Hello World",
                ThreadInfo {
                    depth: 1,
                    is_last: true,
                    ancestors_have_more: vec![false],
                },
            ),
            Message::new(
                "2024-01-13",
                "charlie@example.com",
                "Project Update",
                ThreadInfo {
                    depth: 0,
                    is_last: false,
                    ancestors_have_more: Vec::new(),
                },
            ),
            Message::new(
                "2024-01-12",
                "dave@example.com",
                "Re: Project Update",
                ThreadInfo {
                    depth: 1,
                    is_last: true,
                    ancestors_have_more: vec![false],
                },
            ),
            Message::new(
                "2024-01-11",
                "eve@example.com",
                "Re: Project Update (part 2)",
                ThreadInfo {
                    depth: 2,
                    is_last: true,
                    ancestors_have_more: vec![false, false],
                },
            ),
        ],
    );

    let sent = InMemoryMailbox::new(
        "Sent",
        vec![
            Message::new(
                "2024-01-15",
                "me@example.com",
                "Re: Hello World",
                ThreadInfo {
                    depth: 0,
                    is_last: false,
                    ancestors_have_more: Vec::new(),
                },
            ),
            Message::new(
                "2024-01-14",
                "me@example.com",
                "Meeting Tomorrow",
                ThreadInfo {
                    depth: 0,
                    is_last: false,
                    ancestors_have_more: Vec::new(),
                },
            ),
            Message::new(
                "2024-01-10",
                "me@example.com",
                "Status Update",
                ThreadInfo {
                    depth: 0,
                    is_last: true,
                    ancestors_have_more: Vec::new(),
                },
            ),
        ],
    );

    let drafts = InMemoryMailbox::new(
        "Drafts",
        vec![
            Message::new(
                "2024-01-15",
                "me@example.com",
                "Draft: Proposal",
                ThreadInfo {
                    depth: 0,
                    is_last: false,
                    ancestors_have_more: Vec::new(),
                },
            ),
            Message::new(
                "2024-01-12",
                "me@example.com",
                "Draft: Notes",
                ThreadInfo {
                    depth: 0,
                    is_last: true,
                    ancestors_have_more: Vec::new(),
                },
            ),
        ],
    );

    let trash = InMemoryMailbox::new(
        "Trash",
        vec![Message::new(
            "2024-01-08",
            "spam@example.com",
            "You've won!",
            ThreadInfo {
                depth: 0,
                is_last: true,
                ancestors_have_more: Vec::new(),
            },
        )],
    );

    let archive = InMemoryMailbox::new(
        "Archive",
        vec![
            Message::new(
                "2023-12-20",
                "team@example.com",
                "Holiday Schedule",
                ThreadInfo {
                    depth: 0,
                    is_last: false,
                    ancestors_have_more: Vec::new(),
                },
            ),
            Message::new(
                "2023-12-15",
                "hr@example.com",
                "Year End Review",
                ThreadInfo {
                    depth: 0,
                    is_last: false,
                    ancestors_have_more: Vec::new(),
                },
            ),
            Message::new(
                "2023-11-01",
                "admin@example.com",
                "System Maintenance",
                ThreadInfo {
                    depth: 0,
                    is_last: true,
                    ancestors_have_more: Vec::new(),
                },
            ),
        ],
    );

    vec![
        Box::new(inbox) as Box<dyn Mailbox>,
        Box::new(sent),
        Box::new(drafts),
        Box::new(trash),
        Box::new(archive),
    ]
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

fn thread_prefix(thread: &ThreadInfo) -> String {
    let mut prefix = String::new();
    for has_more in thread.ancestors_have_more.iter().skip(1) {
        if *has_more {
            prefix.push_str("│ ");
        } else {
            prefix.push_str("  ");
        }
    }
    if thread.depth > 0 {
        if thread.is_last {
            prefix.push_str("└─ ");
        } else {
            prefix.push_str("├─ ");
        }
    }
    prefix
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

            let prefix = thread_prefix(&msg.thread);
            let line = format!("{} | {:20} | {}{}", msg.date, msg.from, prefix, msg.subject);
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

    let message = &data.messages[data.selected_message];
    let lines = vec![
        format!("From: {}", message.from),
        format!("Date: {}", message.date),
        format!("Subject: {}", message.subject),
        String::new(),
        "This is the message body.".to_string(),
        String::new(),
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit.".to_string(),
        "Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.".to_string(),
        String::new(),
        "Best regards,".to_string(),
        "The Sender".to_string(),
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
    fn new(stdout: &mut Stdout, mailboxes: Vec<Box<dyn Mailbox>>) -> Result<Self> {
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
        let title = format!(
            " {} [{}/{}] ",
            mailbox_name,
            render_data.selected_message + 1,
            render_data.messages.len()
        );
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
        let title = format!(
            " Message: {} ",
            render_data.messages[render_data.selected_message].subject
        );
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

fn run_loop(stdout: &mut Stdout, mailboxes: Vec<Box<dyn Mailbox>>) -> Result<()> {
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

/// Run the inbox application with custom mailbox providers.
pub fn run_with_mailboxes(mailboxes: Vec<Box<dyn Mailbox>>) -> Result<()> {
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

/// Run the inbox application with sample data.
pub fn run() -> Result<()> {
    run_with_mailboxes(sample_mailbox_data())
}
