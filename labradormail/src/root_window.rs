//! Root window management.
//!
//! The root window is the top-level window in the hierarchy. It contains
//! the help bar, all dialogs container, and message container.
//! It also manages the main application layout (Sidebar/Index/Pager).

use std::io::Result;
use std::io::Stdout;
use std::io::Write;

use crossterm::cursor::Hide;
use crossterm::cursor::Show;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableMouseCapture;

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
use crate::help_data::HelpData;
use crate::message_window::MessageWindow;

use crate::reflow::window_reflow;
use crate::window::WindowId;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowTree;
use crate::window::WindowType;

/// Configuration options for the root window and main layout.
#[derive(Debug, Clone, Copy)]
pub struct RootWindowConfig {
    /// Whether status/help bars should appear at the top.
    pub status_on_top: bool,
    /// Width of the sidebar in columns.
    pub sidebar_width: i16,
    /// Height of status bars in rows.
    pub status_height: i16,
}

impl Default for RootWindowConfig {
    fn default() -> Self {
        Self {
            status_on_top: false,
            sidebar_width: 20,
            status_height: 1,
        }
    }
}

/// Root window manager.
///
/// Manages the top-level window hierarchy including initialization
/// and cleanup of the terminal.
pub struct RootWindow {
    tree: WindowTree,
    root: WindowId,
    help_bar: WindowId,
    dialog_stack: WindowId,
    message_container: WindowId,
    message_base: WindowId,
    /// Whether status bar should appear at top (config option).
    status_on_top: bool,

    // Main Layout Windows
    pub main_layout: WindowId,
    pub sidebar: WindowId,
    pub index_window: WindowId,
    pub index_bar: WindowId,
    pub pager_window: WindowId,
    pub pager_bar: WindowId,
}

impl RootWindow {
    /// Creates a new root window and initializes the terminal.
    pub fn new(stdout: &mut Stdout, help_data: Option<HelpData>) -> Result<Self> {
        Self::new_with_config(stdout, RootWindowConfig::default(), help_data)
    }

    /// Creates a new root window with an explicit configuration.
    pub fn new_with_config(
        stdout: &mut Stdout,
        config: RootWindowConfig,
        help_data: Option<HelpData>,
    ) -> Result<Self> {
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
            help_data,
        )
    }

    /// Creates a new root window with a specific size (for testing).
    pub fn new_with_size(size: (u16, u16)) -> Result<Self> {
        Self::new_with_size_and_config(size, RootWindowConfig::default(), None)
    }

    /// Creates a new root window with a specific size and configuration (for testing).
    pub fn new_with_size_and_config(
        size: (u16, u16),
        config: RootWindowConfig,
        help_data: Option<HelpData>,
    ) -> Result<Self> {
        let (cols, rows) = size;

        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            cols as i16,
            rows as i16,
        );

        // Help bar at top (1 row)
        let help_bar = HelpBar::new(&mut tree);
        let help_bar_window = help_bar.window_id();

        // All dialogs container (takes remaining space)
        let dialog_stack = tree.add_window(
            WindowType::AllDialogs,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        // Message container
        let message_container = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Minimise,
            cols as i16,
            1,
        );

        let msgwin = MessageWindow::new(&mut tree, false);
        let message_base = msgwin.window_id();
        tree.stack_push(message_container, message_base);

        {
            let mut root_layout = tree.layout(root);
            if config.status_on_top {
                root_layout.extend(&[dialog_stack, help_bar_window]);
            } else {
                root_layout.extend(&[help_bar_window, dialog_stack]);
            }
            root_layout.add_existing(message_container);
        }

        // --- Build Main Layout (Sidebar/Index/Pager) ---
        // Using WindowType::DlgIndex as the main container type
        let main_layout = tree.add_dialog(WindowType::DlgIndex);
        {
            let borrowed = tree.get_mut(main_layout);
            borrowed.help_data = help_data;
        }

        let mut sidebar = None;
        let mut index_window = None;
        let mut index_bar = None;
        let mut pager_window = None;
        let mut pager_bar = None;

        {
            let mut main_layout_builder = tree.layout(main_layout);
            main_layout_builder.row_container(WindowSize::Maximise, 0, 0, |row| {
                sidebar = Some(row.window(
                    WindowType::Sidebar,
                    WindowOrientation::Vertical,
                    WindowSize::Fixed,
                    config.sidebar_width,
                    0,
                ));
                row.column_container(WindowSize::Maximise, 0, 0, |main_panel| {
                    main_panel.column_container(WindowSize::Maximise, 0, 0, |index_layout| {
                        index_window = Some(index_layout.window(
                            WindowType::Index,
                            WindowOrientation::Vertical,
                            WindowSize::Maximise,
                            0,
                            0,
                        ));
                        index_bar = Some(index_layout.window(
                            WindowType::StatusBar,
                            WindowOrientation::Vertical,
                            WindowSize::Fixed,
                            0,
                            config.status_height,
                        ));
                    });
                    main_panel.column_container(WindowSize::Maximise, 0, 0, |pager_layout| {
                        pager_window = Some(pager_layout.window(
                            WindowType::Pager,
                            WindowOrientation::Vertical,
                            WindowSize::Maximise,
                            0,
                            0,
                        ));
                        pager_bar = Some(pager_layout.window(
                            WindowType::StatusBar,
                            WindowOrientation::Vertical,
                            WindowSize::Fixed,
                            0,
                            config.status_height,
                        ));
                    });
                });
            });
        }

        let sidebar = sidebar.expect("sidebar window");
        let index_window = index_window.expect("index window");
        let index_bar = index_bar.expect("index bar");
        let pager_window = pager_window.expect("pager window");
        let pager_bar = pager_bar.expect("pager bar");

        // Push Main Layout to Dialog Stack
        tree.stack_push(dialog_stack, main_layout);
        tree.set_focus(main_layout);

        window_reflow(&mut tree, root);

        Ok(Self {
            tree,
            root,
            help_bar: help_bar_window,
            dialog_stack,
            message_container,
            message_base,
            status_on_top: config.status_on_top,

            main_layout,
            sidebar,
            index_window,
            index_bar,
            pager_window,
            pager_bar,
        })
    }

    /// Returns the root window ID.
    pub fn root_id(&self) -> WindowId {
        self.root
    }

    /// Returns the help bar window ID.
    pub fn help_bar_id(&self) -> WindowId {
        self.help_bar
    }

    /// Returns the all dialogs container ID.
    pub fn all_dialogs_id(&self) -> WindowId {
        self.dialog_stack
    }

    /// Returns the message container ID.
    pub fn message_container_id(&self) -> WindowId {
        self.message_container
    }

    /// Returns the base message window ID.
    pub fn message_window_id(&self) -> WindowId {
        self.message_base
    }

    /// Returns a reference to the window tree.
    pub fn tree(&self) -> &WindowTree {
        &self.tree
    }

    /// Returns a mutable reference to the window tree.
    pub fn tree_mut(&mut self) -> &mut WindowTree {
        &mut self.tree
    }

    /// Pushes a new message window onto the stack.
    ///
    /// Hides the current top window and shows the new one.
    pub fn msgcont_push(&mut self, win: WindowId, ctx: &mut GuiContext, out: &mut dyn Write) {
        self.tree.stack_push(self.message_container, win);
        let root = self.tree.get_root(self.message_container);
        self.tree.invalidate_all(root);
        let _ = self.tree.redraw(root, ctx, out);
    }

    /// Pops the top message window from the stack.
    ///
    /// Returns the popped window. Shows the new top window.
    pub fn msgcont_pop(&mut self, ctx: &mut GuiContext, out: &mut dyn Write) -> Option<WindowId> {
        if self.msgcont_len() <= 1 {
            return None;
        }

        let popped = self.tree.stack_pop(self.message_container);
        let root = self.tree.get_root(self.message_container);
        self.tree.invalidate_all(root);
        let _ = self.tree.redraw(root, ctx, out);
        popped
    }

    /// Returns the top (visible) message window.
    pub fn msgcont_top(&self) -> Option<WindowId> {
        self.tree.stack_top(self.message_container)
    }

    /// Returns the base message window (like `msgcont_get_msgwin`).
    ///
    /// This always returns the first (base) message window in the stack.
    pub fn msgcont_get_msgwin(&self) -> WindowId {
        self.message_base
    }

    /// Hides the top message window temporarily.
    pub fn msgcont_hide_top(&mut self) {
        self.tree.stack_hide_top(self.message_container);
    }

    /// Shows the top message window.
    pub fn msgcont_show_top(&mut self) {
        self.tree.stack_show_top(self.message_container);
    }

    /// Returns the number of message windows in the stack.
    pub fn msgcont_len(&self) -> usize {
        self.tree.stack_len(self.message_container)
    }

    /// Pushes a dialog onto the dialog stack.
    pub fn dialog_push(&mut self, window: WindowId) {
        self.tree.stack_push(self.dialog_stack, window);
    }

    /// Pops the top dialog from the dialog stack.
    pub fn dialog_pop(&mut self) -> Option<WindowId> {
        self.tree.stack_pop(self.dialog_stack)
    }

    /// Returns the top dialog.
    pub fn dialog_top(&self) -> Option<WindowId> {
        self.tree.stack_top(self.dialog_stack)
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
        self.tree.swap(self.root, self.help_bar, self.dialog_stack);
        window_reflow(&mut self.tree, self.root);
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
        let root = self.tree.get_mut(self.root);
        root.state.rect.size.cols = cols as i16;
        root.state.rect.size.rows = rows as i16;
        root.set_req_size(cols as i16, rows as i16);

        window_reflow(&mut self.tree, self.root);
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
        self.tree.invalidate_all(self.root);
    }

    /// Redraws the entire window tree.
    pub fn redraw(&mut self, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
        self.tree.redraw(self.root, ctx, out)
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

/// Refreshes screen size using terminal dimensions with LINES/COLUMNS fallback.
pub fn resize_screen(ctx: &mut GuiContext, root: &mut RootWindow) -> Result<()> {
    let (cols, rows) = size().unwrap_or((0, 0));
    apply_resize(ctx, root, cols, rows);
    Ok(())
}

fn apply_resize(ctx: &mut GuiContext, root: &mut RootWindow, cols: u16, rows: u16) {
    let (cols, rows) = resolve_screen_size(cols, rows);
    root.set_size(cols, rows);
    let root_id = root.root_id();
    ctx.register_root_window(root_id);
    root.tree_mut().notify_all(root_id);
    root.tree_mut().invalidate_all(root_id);
}

fn resolve_screen_size(cols: u16, rows: u16) -> (u16, u16) {
    let mut cols = cols;
    let mut rows = rows;

    if rows == 0 {
        rows = std::env::var("LINES")
            .ok()
            .and_then(|val| val.parse::<u16>().ok())
            .unwrap_or(24);
    }

    if cols == 0 {
        cols = std::env::var("COLUMNS")
            .ok()
            .and_then(|val| val.parse::<u16>().ok())
            .unwrap_or(80);
    }

    (cols, rows)
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

    #[test]
    fn root_window_hierarchy() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        tree.get_mut(root).state.rect.size.cols = 80;
        tree.get_mut(root).state.rect.size.rows = 24;

        let help_bar = tree.add_window(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );

        let all_dialogs = tree.add_window(
            WindowType::AllDialogs,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        let message_container = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );

        tree.add_child(root, help_bar);
        tree.add_child(root, all_dialogs);
        tree.add_child(root, message_container);

        window_reflow(&mut tree, root);

        assert_eq!(tree.get(help_bar).state.rect.size.rows, 1);
        assert_eq!(tree.get(help_bar).state.rect.origin.row, 0);

        assert_eq!(tree.get(all_dialogs).state.rect.size.rows, 22);
        assert_eq!(tree.get(all_dialogs).state.rect.origin.row, 1);

        assert_eq!(tree.get(message_container).state.rect.size.rows, 1);
        assert_eq!(tree.get(message_container).state.rect.origin.row, 23);
    }

    #[test]
    fn root_window_with_message_window() {
        let root_win = RootWindow::new_with_size((80, 24)).unwrap();

        // Message window should be in the message container
        let msg_container = root_win.message_container_id();
        assert_eq!(root_win.tree().get(msg_container).children.len(), 1);

        // Message window should be accessible
        let msg_win = root_win.message_window_id();
        assert_eq!(
            root_win.tree().get(msg_win).window_type,
            WindowType::Message
        );
    }

    #[test]
    fn root_window_status_on_top() {
        let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();

        // Initially, help bar is at position 0, all dialogs at position 1
        {
            let root = root_win.root_id();
            let help_bar = root_win.help_bar_id();
            let all_dialogs = root_win.all_dialogs_id();
            assert_eq!(root_win.tree().get(root).children[0], help_bar);
            assert_eq!(root_win.tree().get(root).children[1], all_dialogs);
        }

        // Set status_on_top
        root_win.set_status_on_top(true);

        // Now all dialogs should be at position 0, help bar at position 1
        {
            let root = root_win.root_id();
            let help_bar = root_win.help_bar_id();
            let all_dialogs = root_win.all_dialogs_id();
            assert_eq!(root_win.tree().get(root).children[0], all_dialogs);
            assert_eq!(root_win.tree().get(root).children[1], help_bar);
        }
    }

    #[test]
    fn root_window_initial_status_on_top() {
        let root_win = RootWindow::new_with_size_and_config(
            (80, 24),
            RootWindowConfig {
                status_on_top: true,
                ..Default::default()
            },
            None,
        )
        .unwrap();

        let root = root_win.root_id();
        let help_bar = root_win.help_bar_id();
        let all_dialogs = root_win.all_dialogs_id();
        assert_eq!(root_win.tree().get(root).children[0], all_dialogs);
        assert_eq!(root_win.tree().get(root).children[1], help_bar);
        assert!(root_win.status_on_top());
    }

    #[test]
    fn root_window_resize() {
        let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();

        // Initial size
        {
            let root = root_win.root_id();
            assert_eq!(root_win.tree().get(root).state.rect.size.cols, 80);
            assert_eq!(root_win.tree().get(root).state.rect.size.rows, 24);
        }

        // Resize
        root_win.handle_resize(100, 50);

        {
            let root = root_win.root_id();
            assert_eq!(root_win.tree().get(root).state.rect.size.cols, 100);
            assert_eq!(root_win.tree().get(root).state.rect.size.rows, 50);
        }

        // All children should be marked for repaint
        assert!(root_win
            .tree()
            .get(root_win.help_bar_id())
            .actions
            .contains(WindowActionFlags::REPAINT));
    }

    #[test]
    fn msgcont_push_pop() {
        let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();

        // Initially, just the base message window
        assert_eq!(root_win.msgcont_len(), 1);

        // Push a new message window
        let new_msgwin = MessageWindow::new(root_win.tree_mut(), true);
        root_win.msgcont_push(new_msgwin.window_id(), &mut ctx, &mut out);

        assert_eq!(root_win.msgcont_len(), 2);

        // Base window should be hidden, new one visible
        assert!(
            !root_win
                .tree()
                .get(root_win.message_window_id())
                .state
                .visible
        );
        assert!(root_win.tree().get(new_msgwin.window_id()).state.visible);

        // Top should be the new one
        let top = root_win.msgcont_top().unwrap();
        assert_eq!(top, new_msgwin.window_id());

        // Pop
        let popped = root_win.msgcont_pop(&mut ctx, &mut out);
        assert!(popped.is_some());
        assert_eq!(root_win.msgcont_len(), 1);

        // Base window should be visible again
        assert!(
            root_win
                .tree()
                .get(root_win.message_window_id())
                .state
                .visible
        );
    }

    #[test]
    fn msgcont_cannot_pop_base() {
        let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();

        // Only base window present
        assert_eq!(root_win.msgcont_len(), 1);

        // Attempt to pop should return None
        let popped = root_win.msgcont_pop(&mut ctx, &mut out);
        assert!(popped.is_none());
        assert_eq!(root_win.msgcont_len(), 1);
    }
}
