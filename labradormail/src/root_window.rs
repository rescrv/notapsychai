//! Root window management.
//!
//! The root window is the top-level window in the hierarchy. It contains
//! the help bar, all dialogs container, and message container.

use std::cell::RefCell;
use std::io::Result;
use std::io::Stdout;
use std::io::Write;
use std::rc::Rc;

use crossterm::cursor::Hide;
use crossterm::cursor::Show;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableMouseCapture;
use crossterm::event::Event;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use crossterm::terminal::size;
use crossterm::terminal::Clear;
use crossterm::terminal::ClearType;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use crossterm::ExecutableCommand;

use crate::context::GuiContext;
use crate::help_bar::HelpBar;
use crate::msgcont::MessageContainer;
use crate::terminal::Terminal;
use crate::window::MuttWindow;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowType;
use crate::window_reflow;

/// Redraws the root window from the context.
pub fn redraw_global(ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
    if let Some(root) = ctx.root_window() {
        MuttWindow::invalidate_all(&root);
        MuttWindow::redraw(&root, ctx, out)?;
    }
    Ok(())
}

/// Configuration options for the root window.
#[derive(Debug, Clone, Copy, Default)]
pub struct RootWindowConfig {
    /// Whether status/help bars should appear at the top.
    pub status_on_top: bool,
}

/// Root window manager.
///
/// Manages the top-level window hierarchy including initialization
/// and cleanup of the terminal.
pub struct RootWindow {
    root: Rc<RefCell<MuttWindow>>,
    help_bar: Rc<RefCell<MuttWindow>>,
    all_dialogs: Rc<RefCell<MuttWindow>>,
    message_container: MessageContainer,
    /// Whether status bar should appear at top (config option).
    status_on_top: bool,
}

impl RootWindow {
    /// Creates a new root window and initializes the terminal.
    pub fn new(stdout: &mut Stdout) -> Result<Self> {
        Self::new_with_config(stdout, RootWindowConfig::default())
    }

    /// Creates a new root window with an explicit configuration.
    pub fn new_with_config(stdout: &mut Stdout, config: RootWindowConfig) -> Result<Self> {
        enable_raw_mode()
            .map_err(|e| std::io::Error::other(format!("failed to enable raw mode: {e}")))?;
        stdout
            .execute(EnterAlternateScreen)
            .map_err(|e| std::io::Error::other(format!("failed to enter alternate screen: {e}")))?;
        stdout
            .execute(EnableMouseCapture)
            .map_err(|e| std::io::Error::other(format!("failed to enable mouse capture: {e}")))?;
        stdout
            .execute(Hide)
            .map_err(|e| std::io::Error::other(format!("failed to hide cursor: {e}")))?;
        stdout
            .execute(Clear(ClearType::All))
            .map_err(|e| std::io::Error::other(format!("failed to clear screen: {e}")))?;

        Self::new_with_size_and_config(
            size()
                .map_err(|e| std::io::Error::other(format!("failed to get terminal size: {e}")))?,
            config,
        )
    }

    /// Creates a new root window with a specific size (for testing).
    pub fn new_with_size(size: (u16, u16)) -> Result<Self> {
        Self::new_with_size_and_config(size, RootWindowConfig::default())
    }

    /// Creates a new root window with a specific size and configuration (for testing).
    pub fn new_with_size_and_config(size: (u16, u16), config: RootWindowConfig) -> Result<Self> {
        let (cols, rows) = size;

        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            cols as i16,
            rows as i16,
        );

        // Help bar at top (1 row)
        let help_bar = HelpBar::new();
        let help_bar_window = Rc::clone(help_bar.window());

        // All dialogs container (takes remaining space)
        let all_dialogs = MuttWindow::new(
            WindowType::AllDialogs,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        let message_container = MessageContainer::new(cols as i16);

        if config.status_on_top {
            MuttWindow::add_child(&root, Rc::clone(&all_dialogs));
            MuttWindow::add_child(&root, Rc::clone(&help_bar_window));
        } else {
            MuttWindow::add_child(&root, Rc::clone(&help_bar_window));
            MuttWindow::add_child(&root, Rc::clone(&all_dialogs));
        }
        MuttWindow::add_child(&root, Rc::clone(message_container.window()));

        window_reflow(&root);

        Ok(Self {
            root,
            help_bar: help_bar_window,
            all_dialogs,
            message_container,
            status_on_top: config.status_on_top,
        })
    }

    /// Returns a reference to the root window.
    pub fn root(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.root
    }

    /// Returns a reference to the help bar window.
    pub fn help_bar(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.help_bar
    }

    /// Returns a reference to the all dialogs container.
    pub fn all_dialogs(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.all_dialogs
    }

    /// Returns a reference to the message container.
    pub fn message_container(&self) -> &Rc<RefCell<MuttWindow>> {
        self.message_container.window()
    }

    /// Returns a reference to the base message window.
    ///
    /// This is the default message window at the bottom of the stack.
    /// Use `msgcont_get_msgwin` to get the currently active (top) message window.
    pub fn message_window(&self) -> &Rc<RefCell<MuttWindow>> {
        self.message_container.base_window()
    }

    /// Pushes a new message window onto the stack.
    ///
    /// Hides the current top window and shows the new one.
    pub fn msgcont_push(
        &self,
        win: Rc<RefCell<MuttWindow>>,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) {
        self.message_container.push(win, Some(ctx), Some(out));
    }

    /// Pops the top message window from the stack.
    ///
    /// Returns the popped window. Shows the new top window.
    pub fn msgcont_pop(
        &self,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Option<Rc<RefCell<MuttWindow>>> {
        self.message_container.pop(Some(ctx), Some(out))
    }

    /// Returns the top (visible) message window.
    pub fn msgcont_top(&self) -> Option<Rc<RefCell<MuttWindow>>> {
        self.message_container.top()
    }

    /// Returns the base message window (like `msgcont_get_msgwin`).
    ///
    /// This always returns the first (base) message window in the stack.
    pub fn msgcont_get_msgwin(&self) -> &Rc<RefCell<MuttWindow>> {
        self.message_container.get_msgwin()
    }

    /// Hides the top message window temporarily.
    pub fn msgcont_hide_top(&self) {
        self.message_container.hide_top();
    }

    /// Shows the top message window.
    pub fn msgcont_show_top(&self) {
        self.message_container.show_top();
    }

    /// Returns the number of message windows in the stack.
    pub fn msgcont_len(&self) -> usize {
        self.message_container.len()
    }

    /// Returns the current status_on_top setting.
    pub fn status_on_top(&self) -> bool {
        self.status_on_top
    }

    /// Sets the status_on_top config option.
    ///
    /// When true, swaps the help bar and all dialogs container
    /// to put the help bar at the bottom.
    pub fn set_status_on_top(&mut self, on_top: bool) {
        if self.status_on_top == on_top {
            return;
        }
        self.status_on_top = on_top;

        // Swap help bar and all dialogs container positions
        self.root
            .borrow_mut()
            .swap(&self.help_bar, &self.all_dialogs);
        window_reflow(&self.root);
    }

    /// Applies a status_on_top config change.
    pub fn handle_status_on_top_change(&mut self, on_top: bool) {
        self.set_status_on_top(on_top);
    }

    /// Applies root window configuration updates.
    pub fn apply_config(&mut self, config: RootWindowConfig) {
        self.set_status_on_top(config.status_on_top);
    }

    /// Updates the root window size (call on terminal resize).
    pub fn set_size(&mut self, cols: u16, rows: u16) {
        let mut root = self.root.borrow_mut();
        root.state.cols = cols as i16;
        root.state.rows = rows as i16;
        root.req_cols = cols as i16;
        root.req_rows = rows as i16;
        drop(root);

        window_reflow(&self.root);
    }

    /// Refreshes the size from the terminal.
    pub fn refresh_size(&mut self) -> Result<()> {
        let (cols, rows) = size()
            .map_err(|e| std::io::Error::other(format!("failed to get terminal size: {e}")))?;
        self.set_size(cols, rows);
        Ok(())
    }

    /// Handles a resize event (SIGWINCH-equivalent).
    ///
    /// Call this when receiving a terminal resize event.
    /// Invalidates all windows and triggers a full reflow.
    pub fn handle_resize(&mut self, cols: u16, rows: u16) {
        self.set_size(cols, rows);
        MuttWindow::invalidate_all(&self.root);
    }

    /// Handles a resize event by invalidating and reflowing windows.
    pub fn handle_resize_event(&mut self, event: &Event) -> bool {
        if let Some((cols, rows)) = Terminal::get_resize_dimensions(event) {
            self.handle_resize(cols, rows);
            return true;
        }
        false
    }

    /// Redraws the entire window tree.
    pub fn redraw(&self, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
        MuttWindow::redraw(&self.root, ctx, out)
    }

    /// Shows the cursor.
    pub fn show_cursor(out: &mut dyn Write) -> Result<()> {
        out.execute(Show)
            .map_err(|e| std::io::Error::other(format!("failed to show cursor: {e}")))?;
        Ok(())
    }

    /// Hides the cursor.
    pub fn hide_cursor(out: &mut dyn Write) -> Result<()> {
        out.execute(Hide)
            .map_err(|e| std::io::Error::other(format!("failed to hide cursor: {e}")))?;
        Ok(())
    }

    /// Cleans up the terminal.
    pub fn cleanup(stdout: &mut dyn Write) -> Result<()> {
        stdout
            .execute(Show)
            .map_err(|e| std::io::Error::other(format!("failed to show cursor: {e}")))?;
        stdout
            .execute(DisableMouseCapture)
            .map_err(|e| std::io::Error::other(format!("failed to disable mouse capture: {e}")))?;
        stdout
            .execute(LeaveAlternateScreen)
            .map_err(|e| std::io::Error::other(format!("failed to leave alternate screen: {e}")))?;
        disable_raw_mode()
            .map_err(|e| std::io::Error::other(format!("failed to disable raw mode: {e}")))?;
        stdout.flush()?;
        Ok(())
    }
}

impl Drop for RootWindow {
    fn drop(&mut self) {
        // Best effort cleanup
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_window::MessageWindow;
    use crate::window::WindowActionFlags;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    static RECALC_COUNT: AtomicUsize = AtomicUsize::new(0);
    static DRAW_COUNT: AtomicUsize = AtomicUsize::new(0);

    fn test_recalc(win: &mut MuttWindow) {
        RECALC_COUNT.fetch_add(1, Ordering::SeqCst);
        win.actions |= WindowActionFlags::REPAINT;
    }

    fn test_draw(_win: &mut MuttWindow, _ctx: &mut GuiContext, _out: &mut dyn Write) -> Result<()> {
        DRAW_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    #[test]
    fn root_window_hierarchy() {
        // Create a mock root window without terminal initialization
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let help_bar = MuttWindow::new(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );

        let all_dialogs = MuttWindow::new(
            WindowType::AllDialogs,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        let message_container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );

        MuttWindow::add_child(&root, Rc::clone(&help_bar));
        MuttWindow::add_child(&root, Rc::clone(&all_dialogs));
        MuttWindow::add_child(&root, Rc::clone(&message_container));

        window_reflow(&root);

        assert_eq!(help_bar.borrow().state.rows, 1);
        assert_eq!(help_bar.borrow().state.row_offset, 0);

        assert_eq!(all_dialogs.borrow().state.rows, 22);
        assert_eq!(all_dialogs.borrow().state.row_offset, 1);

        assert_eq!(message_container.borrow().state.rows, 1);
        assert_eq!(message_container.borrow().state.row_offset, 23);
    }

    #[test]
    fn root_window_with_message_window() {
        let root_win = RootWindow::new_with_size((80, 24)).unwrap();

        // Message window should be in the message container
        let msg_container = root_win.message_container();
        assert_eq!(msg_container.borrow().children.len(), 1);

        // Message window should be accessible
        let msg_win = root_win.message_window();
        assert_eq!(msg_win.borrow().window_type, WindowType::Message);
    }

    #[test]
    fn root_window_status_on_top() {
        let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();

        // Initially, help bar is at position 0, all dialogs at position 1
        {
            let root = root_win.root();
            let help_bar = root_win.help_bar();
            let all_dialogs = root_win.all_dialogs();
            assert!(MuttWindow::same_window(
                &root.borrow().children[0],
                help_bar
            ));
            assert!(MuttWindow::same_window(
                &root.borrow().children[1],
                all_dialogs
            ));
        }

        // Set status_on_top
        root_win.set_status_on_top(true);

        // Now all dialogs should be at position 0, help bar at position 1
        {
            let root = root_win.root();
            let help_bar = root_win.help_bar();
            let all_dialogs = root_win.all_dialogs();
            assert!(MuttWindow::same_window(
                &root.borrow().children[0],
                all_dialogs
            ));
            assert!(MuttWindow::same_window(
                &root.borrow().children[1],
                help_bar
            ));
        }
    }

    #[test]
    fn root_window_initial_status_on_top() {
        let root_win = RootWindow::new_with_size_and_config(
            (80, 24),
            RootWindowConfig {
                status_on_top: true,
            },
        )
        .unwrap();

        let root = root_win.root();
        let help_bar = root_win.help_bar();
        let all_dialogs = root_win.all_dialogs();
        assert!(MuttWindow::same_window(
            &root.borrow().children[0],
            all_dialogs
        ));
        assert!(MuttWindow::same_window(
            &root.borrow().children[1],
            help_bar
        ));
        assert!(root_win.status_on_top());
    }

    #[test]
    fn root_window_resize() {
        let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();

        // Initial size
        {
            let root = root_win.root();
            assert_eq!(root.borrow().state.cols, 80);
            assert_eq!(root.borrow().state.rows, 24);
        }

        // Resize
        root_win.handle_resize(100, 50);

        {
            let root = root_win.root();
            assert_eq!(root.borrow().state.cols, 100);
            assert_eq!(root.borrow().state.rows, 50);
        }

        // All children should be marked for repaint
        assert!(root_win
            .help_bar()
            .borrow()
            .actions
            .contains(WindowActionFlags::REPAINT));
    }

    #[test]
    fn msgcont_push_pop() {
        let root_win = RootWindow::new_with_size((80, 24)).unwrap();
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();

        // Initially, just the base message window
        assert_eq!(root_win.msgcont_len(), 1);

        // Push a new message window
        let new_msgwin = MessageWindow::new(true);
        root_win.msgcont_push(Rc::clone(new_msgwin.window()), &mut ctx, &mut out);

        assert_eq!(root_win.msgcont_len(), 2);

        // Base window should be hidden, new one visible
        assert!(!root_win.message_window().borrow().state.visible);
        assert!(new_msgwin.window().borrow().state.visible);

        // Top should be the new one
        let top = root_win.msgcont_top().unwrap();
        assert!(MuttWindow::same_window(&top, new_msgwin.window()));

        // Pop
        let popped = root_win.msgcont_pop(&mut ctx, &mut out);
        assert!(popped.is_some());
        assert_eq!(root_win.msgcont_len(), 1);

        // Base window should be visible again
        assert!(root_win.message_window().borrow().state.visible);
    }

    #[test]
    fn msgcont_cannot_pop_base() {
        let root_win = RootWindow::new_with_size((80, 24)).unwrap();
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();

        assert_eq!(root_win.msgcont_len(), 1);

        // Cannot pop the base message window
        let popped = root_win.msgcont_pop(&mut ctx, &mut out);
        assert!(popped.is_none());
        assert_eq!(root_win.msgcont_len(), 1);
    }

    #[test]
    fn msgcont_hide_show() {
        let root_win = RootWindow::new_with_size((80, 24)).unwrap();

        // Initially visible
        assert!(root_win.message_window().borrow().state.visible);

        // Hide
        root_win.msgcont_hide_top();
        assert!(!root_win.message_window().borrow().state.visible);

        // Show
        root_win.msgcont_show_top();
        assert!(root_win.message_window().borrow().state.visible);
    }

    #[test]
    fn msgcont_get_msgwin() {
        let root_win = RootWindow::new_with_size((80, 24)).unwrap();
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();

        // msgcont_get_msgwin always returns the base message window
        let base = root_win.msgcont_get_msgwin();
        assert!(MuttWindow::same_window(base, root_win.message_window()));

        // Push a new one
        let new_msgwin = MessageWindow::new(true);
        root_win.msgcont_push(Rc::clone(new_msgwin.window()), &mut ctx, &mut out);

        // Still returns base
        let base_after = root_win.msgcont_get_msgwin();
        assert!(MuttWindow::same_window(
            base_after,
            root_win.message_window()
        ));
    }

    #[test]
    fn root_window_accessors() {
        let root_win = RootWindow::new_with_size((80, 24)).unwrap();
        let root = root_win.root();
        let help_bar = root_win.help_bar();
        let all_dialogs = root_win.all_dialogs();
        let msg_container = root_win.message_container();

        let children = &root.borrow().children;
        assert!(children
            .iter()
            .any(|child| MuttWindow::same_window(child, help_bar)));
        assert!(children
            .iter()
            .any(|child| MuttWindow::same_window(child, all_dialogs)));
        assert!(children
            .iter()
            .any(|child| MuttWindow::same_window(child, msg_container)));
    }

    #[test]
    fn root_window_set_size_updates_state() {
        let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();
        root_win.set_size(100, 50);

        let root = root_win.root();
        let borrowed = root.borrow();
        assert_eq!(borrowed.state.cols, 100);
        assert_eq!(borrowed.state.rows, 50);
        assert_eq!(borrowed.req_cols, 100);
        assert_eq!(borrowed.req_rows, 50);
    }

    #[test]
    fn redraw_global_no_root_is_ok() {
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        redraw_global(&mut ctx, &mut out).unwrap();
    }

    #[test]
    fn redraw_global_calls_callbacks() {
        let mut ctx = GuiContext::new();
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            5,
        );
        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            5,
        );
        {
            let mut child_mut = child.borrow_mut();
            child_mut.recalc = Some(test_recalc);
            child_mut.draw = Some(test_draw);
            child_mut.actions |= WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
        }
        MuttWindow::add_child(&root, Rc::clone(&child));
        ctx.register_root_window(&root);
        RECALC_COUNT.store(0, Ordering::SeqCst);
        DRAW_COUNT.store(0, Ordering::SeqCst);

        let mut out = Vec::new();
        redraw_global(&mut ctx, &mut out).unwrap();
        assert!(RECALC_COUNT.load(Ordering::SeqCst) > 0);
        assert!(DRAW_COUNT.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn cleanup_returns_ok() {
        let mut out = Vec::new();
        RootWindow::cleanup(&mut out).unwrap();
    }
}
