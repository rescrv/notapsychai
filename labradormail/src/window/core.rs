//! Core window management types and implementation.
//!
//! This module provides the fundamental window structures including `MuttWindow`,
//! window types, orientations, sizing modes, and notification systems.

use super::NotifyWindow;
use super::WindowNotifyFlags;
use super::WindowObserver;
use std::any::Any;
use std::cell::RefCell;
use std::io::Result;
use std::io::Write;
use std::rc::Rc;
use std::rc::Weak;

use bitflags::bitflags;
use crossterm::cursor;
use crossterm::cursor::Hide;
use crossterm::cursor::MoveTo;
use crossterm::style::Print;
use crossterm::ExecutableCommand;

use crate::context::GuiContext;
use crate::help_data::HelpData;
use crate::mutt_curses::mutt_curses_current_color;
use crate::mutt_curses::mutt_curses_set_color;
use crate::reflow::window_reflow;

/// Window orientation for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowOrientation {
    /// Children stack vertically.
    #[default]
    Vertical,
    /// Children stack horizontally.
    Horizontal,
}

/// Window sizing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowSize {
    /// Fixed size (uses req_rows/req_cols).
    #[default]
    Fixed,
    /// Take as much space as possible.
    Maximise,
    /// Shrink to fit children.
    Minimise,
}

/// Window type identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowType {
    /// Root window.
    #[default]
    Root,
    /// Generic container.
    Container,
    /// All dialogs container.
    AllDialogs,

    // Dialogs
    /// Alias dialog.
    DlgAlias,
    /// Attachment dialog.
    DlgAttachment,
    /// Autocrypt dialog.
    DlgAutocrypt,
    /// Browser dialog.
    DlgBrowser,
    /// Certificate dialog.
    DlgCertificate,
    /// Compose dialog.
    DlgCompose,
    /// GPGME dialog.
    DlgGpgme,
    /// Pager dialog.
    DlgPager,
    /// History dialog.
    DlgHistory,
    /// Help dialog.
    DlgHelp,
    /// Index dialog.
    DlgIndex,
    /// Pattern dialog.
    DlgPattern,
    /// PGP dialog.
    DlgPgp,
    /// Postponed dialog.
    DlgPostponed,
    /// Query dialog.
    DlgQuery,
    /// S/MIME dialog.
    DlgSmime,

    // Common windows
    /// Custom window.
    Custom,
    /// Help bar.
    HelpBar,
    /// Index window.
    Index,
    /// Menu window.
    Menu,
    /// Message window.
    Message,
    /// Pager window.
    Pager,
    /// Sidebar window.
    Sidebar,
    /// Status bar window.
    StatusBar,
}

impl WindowType {
    /// Returns the name of this window type for debugging.
    pub fn name(&self) -> &'static str {
        match self {
            WindowType::Root => "Root",
            WindowType::Container => "Container",
            WindowType::AllDialogs => "AllDialogs",
            WindowType::DlgAlias => "DlgAlias",
            WindowType::DlgAttachment => "DlgAttachment",
            WindowType::DlgAutocrypt => "DlgAutocrypt",
            WindowType::DlgBrowser => "DlgBrowser",
            WindowType::DlgCertificate => "DlgCertificate",
            WindowType::DlgCompose => "DlgCompose",
            WindowType::DlgGpgme => "DlgGpgme",
            WindowType::DlgPager => "DlgPager",
            WindowType::DlgHistory => "DlgHistory",
            WindowType::DlgHelp => "DlgHelp",
            WindowType::DlgIndex => "DlgIndex",
            WindowType::DlgPattern => "DlgPattern",
            WindowType::DlgPgp => "DlgPgp",
            WindowType::DlgPostponed => "DlgPostponed",
            WindowType::DlgQuery => "DlgQuery",
            WindowType::DlgSmime => "DlgSmime",
            WindowType::Custom => "Custom",
            WindowType::HelpBar => "HelpBar",
            WindowType::Index => "Index",
            WindowType::Menu => "Menu",
            WindowType::Message => "Message",
            WindowType::Pager => "Pager",
            WindowType::Sidebar => "Sidebar",
            WindowType::StatusBar => "StatusBar",
        }
    }

    /// Returns true if this window type is a dialog.
    pub fn is_dialog(&self) -> bool {
        matches!(
            self,
            WindowType::DlgAlias
                | WindowType::DlgAttachment
                | WindowType::DlgAutocrypt
                | WindowType::DlgBrowser
                | WindowType::DlgCertificate
                | WindowType::DlgCompose
                | WindowType::DlgGpgme
                | WindowType::DlgPager
                | WindowType::DlgHistory
                | WindowType::DlgHelp
                | WindowType::DlgIndex
                | WindowType::DlgPattern
                | WindowType::DlgPgp
                | WindowType::DlgPostponed
                | WindowType::DlgQuery
                | WindowType::DlgSmime
        )
    }
}

bitflags! {
    /// Action flags for pending window operations.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct WindowActionFlags: u8 {
        /// No pending actions.
        const NONE = 0;
        /// Reflow window and children.
        const REFLOW = 1 << 0;
        /// Recalculate contents.
        const RECALC = 1 << 1;
        /// Redraw contents.
        const REPAINT = 1 << 2;
    }
}

/// Current state of a window.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowState {
    /// Whether the window is visible.
    pub visible: bool,
    /// Number of columns.
    pub cols: i16,
    /// Number of rows.
    pub rows: i16,
    /// Absolute screen column offset.
    pub col_offset: i16,
    /// Absolute screen row offset.
    pub row_offset: i16,
}

impl WindowState {
    /// Creates a new window state with the given dimensions.
    pub fn new(cols: i16, rows: i16) -> Self {
        Self {
            visible: true,
            cols,
            rows,
            col_offset: 0,
            row_offset: 0,
        }
    }
}

/// Cursor visibility state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CursorState {
    /// Cursor is invisible.
    Invisible = 0,
    /// Cursor is visible.
    Visible = 1,
    /// Cursor is very visible (block cursor).
    VeryVisible = 2,
}

/// Type alias for window callback functions.
pub type WindowCallback = fn(&mut MuttWindow);
/// Type alias for recursor callback functions.
pub type RecursorCallback = fn(&mut MuttWindow, &mut GuiContext, &mut dyn Write) -> bool;
/// Type alias for draw callback functions (stdout-aware).
pub type DrawCallback = fn(&mut MuttWindow, &mut GuiContext, &mut dyn Write) -> Result<()>;
/// Type alias for window data free callback.
pub type WdataFreeCallback = fn(&mut MuttWindow);

/// Main window structure.
///
/// Represents a window in the hierarchical window tree. Windows can contain
/// child windows and are laid out according to their orientation and size mode.
pub struct MuttWindow {
    /// Requested number of columns.
    pub req_cols: i16,
    /// Requested number of rows.
    pub req_rows: i16,
    /// Current window state.
    pub state: WindowState,
    /// Previous window state (for notifications).
    pub old: WindowState,
    /// Layout direction for children.
    pub orient: WindowOrientation,
    /// Sizing mode.
    pub size: WindowSize,
    /// Pending actions.
    pub actions: WindowActionFlags,

    /// Parent window.
    pub parent: Option<Weak<RefCell<MuttWindow>>>,
    /// Child windows.
    pub children: Vec<Rc<RefCell<MuttWindow>>>,
    /// Currently focused child.
    pub focus: Option<Weak<RefCell<MuttWindow>>>,

    /// Window type identifier.
    pub window_type: WindowType,
    /// Private window data.
    pub wdata: Option<Box<dyn Any>>,

    /// Callback to free window data.
    pub wdata_free: Option<WdataFreeCallback>,
    /// Callback to recalculate window contents.
    pub recalc: Option<WindowCallback>,
    /// Callback to repaint window contents.
    pub repaint: Option<WindowCallback>,
    /// Callback to position cursor.
    pub recursor: Option<RecursorCallback>,
    /// Callback to draw window contents.
    pub draw: Option<DrawCallback>,

    /// Help bar data for this window (optional).
    pub help_data: Option<Rc<HelpData>>,

    /// Registered observers for window events.
    pub observers: Vec<WindowObserver>,

    /// Set when `set_focus` is called on an orphan window (no parent).
    /// Cleared when the focus chain is properly established after being added to tree.
    pub(crate) pending_focus: bool,
}

impl std::fmt::Debug for MuttWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MuttWindow")
            .field("req_cols", &self.req_cols)
            .field("req_rows", &self.req_rows)
            .field("state", &self.state)
            .field("orient", &self.orient)
            .field("size", &self.size)
            .field("window_type", &self.window_type)
            .field("children_count", &self.children.len())
            .finish()
    }
}

impl Default for MuttWindow {
    fn default() -> Self {
        Self::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        )
        .borrow()
        .clone_without_rc()
    }
}

impl MuttWindow {
    /// Creates a clone of this window without Rc/RefCell wrapping (for Default impl).
    fn clone_without_rc(&self) -> Self {
        Self {
            req_cols: self.req_cols,
            req_rows: self.req_rows,
            state: self.state,
            old: self.old,
            orient: self.orient,
            size: self.size,
            actions: self.actions,
            parent: None,
            children: Vec::new(),
            focus: None,
            window_type: self.window_type,
            wdata: None,
            wdata_free: None,
            recalc: None,
            repaint: None,
            recursor: None,
            draw: None,
            help_data: None,
            observers: Vec::new(),
            pending_focus: false,
        }
    }

    /// Creates a new window.
    ///
    /// Windows are initialized with RECALC|REPAINT actions set, matching NeoMutt's `mutt_window_new`.
    pub fn new(
        window_type: WindowType,
        orient: WindowOrientation,
        size: WindowSize,
        cols: i16,
        rows: i16,
    ) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            req_cols: cols,
            req_rows: rows,
            state: WindowState::new(cols, rows),
            old: WindowState::default(),
            orient,
            size,
            actions: WindowActionFlags::RECALC | WindowActionFlags::REPAINT,
            parent: None,
            children: Vec::new(),
            focus: None,
            window_type,
            wdata: None,
            wdata_free: None,
            recalc: None,
            repaint: None,
            recursor: None,
            draw: None,
            help_data: None,
            observers: Vec::new(),
            pending_focus: false,
        }))
    }

    /// Returns a typed reference to the window data, if present.
    pub fn wdata_ref<T: 'static>(&self) -> Option<&T> {
        self.wdata.as_deref()?.downcast_ref::<T>()
    }

    /// Returns a typed mutable reference to the window data, if present.
    pub fn wdata_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.wdata.as_deref_mut()?.downcast_mut::<T>()
    }

    /// Adds a child window.
    pub fn add_child(parent: &Rc<RefCell<Self>>, child: Rc<RefCell<MuttWindow>>) {
        child.borrow_mut().parent = Some(Rc::downgrade(parent));
        let child_ref = {
            let mut borrowed = parent.borrow_mut();
            borrowed.children.push(child);
            borrowed.actions |= WindowActionFlags::REFLOW;
            borrowed.children.last().cloned()
        };
        if let Some(ref child_ref) = child_ref {
            Self::notify_with_flags_rc(child_ref, NotifyWindow::Add, WindowNotifyFlags::empty());
        }

        // If the newly added subtree contains a pending focus target (from a prior set_focus
        // call when the window wasn't in the tree), establish the focus chain now.
        if let Some(ref child_ref) = child_ref {
            if let Some(focus_target) = Self::find_pending_focus(child_ref) {
                Self::set_focus(&focus_target);
            }
        }
    }

    /// Removes a child window from this window.
    ///
    /// Also clears focus if the removed window or any of its descendants had focus.
    pub fn remove_child(
        &mut self,
        child: &Rc<RefCell<MuttWindow>>,
    ) -> Option<Rc<RefCell<MuttWindow>>> {
        if let Some(pos) = self.children.iter().position(|c| Rc::ptr_eq(c, child)) {
            let removed = self.children.remove(pos);

            // Clear focus if it pointed to the removed child
            if let Some(ref focus_weak) = self.focus {
                if let Some(focus) = focus_weak.upgrade() {
                    if Rc::ptr_eq(&focus, child) {
                        self.focus = None;
                    }
                }
            }

            Self::notify_with_flags_rc(&removed, NotifyWindow::Delete, WindowNotifyFlags::empty());
            removed.borrow_mut().parent = None;
            self.actions |= WindowActionFlags::REFLOW;
            Some(removed)
        } else {
            None
        }
    }

    /// Checks if this window is visible, including all ancestors.
    pub fn is_visible(&self) -> bool {
        if !self.state.visible {
            return false;
        }
        if let Some(ref parent_weak) = self.parent {
            if let Some(parent) = parent_weak.upgrade() {
                return parent.borrow().is_visible();
            }
        }
        true
    }

    /// Checks if this window was visible, based on old state and ancestors.
    pub fn was_visible(&self) -> bool {
        if !self.old.visible {
            return false;
        }
        if let Some(ref parent_weak) = self.parent {
            if let Some(parent) = parent_weak.upgrade() {
                return parent.borrow().was_visible();
            }
        }
        true
    }

    /// Sets the visibility of this window.
    ///
    /// Note: This only sets the visibility flag. The parent window's reflow
    /// flag should be set separately if needed.
    pub fn set_visible(&mut self, visible: bool) {
        if self.state.visible == visible {
            return;
        }
        self.state.visible = visible;
        self.actions |= WindowActionFlags::REFLOW;
    }

    /// Sets the visibility of a window and marks parent for reflow.
    pub fn set_visible_with_parent(win: &Rc<RefCell<Self>>, visible: bool) {
        let parent_weak = {
            let mut borrowed = win.borrow_mut();
            if borrowed.state.visible == visible {
                return;
            }
            borrowed.state.visible = visible;
            borrowed.actions |= WindowActionFlags::REFLOW;
            borrowed.parent.clone()
        };

        if let Some(parent_weak) = parent_weak {
            if let Some(parent) = parent_weak.upgrade() {
                parent.borrow_mut().actions |= WindowActionFlags::REFLOW;
            }
        }
    }

    /// Finds a child window by type (depth-first search).
    pub fn find_child(
        win: &Rc<RefCell<Self>>,
        window_type: WindowType,
    ) -> Option<Rc<RefCell<MuttWindow>>> {
        let borrowed = win.borrow();
        for child in &borrowed.children {
            if child.borrow().window_type == window_type {
                return Some(Rc::clone(child));
            }
            if let Some(found) = Self::find_child(child, window_type) {
                return Some(found);
            }
        }
        None
    }

    /// Finds a parent window by type.
    pub fn find_parent(
        win: &Rc<RefCell<Self>>,
        window_type: WindowType,
    ) -> Option<Rc<RefCell<MuttWindow>>> {
        let borrowed = win.borrow();
        if let Some(ref parent_weak) = borrowed.parent {
            if let Some(parent) = parent_weak.upgrade() {
                if parent.borrow().window_type == window_type {
                    return Some(parent);
                }
                return Self::find_parent(&parent, window_type);
            }
        }
        None
    }

    /// Gets the root window of the tree containing this window.
    pub fn get_root(win: &Rc<RefCell<Self>>) -> Rc<RefCell<MuttWindow>> {
        let borrowed = win.borrow();
        if let Some(ref parent_weak) = borrowed.parent {
            if let Some(parent) = parent_weak.upgrade() {
                return Self::get_root(&parent);
            }
        }
        Rc::clone(win)
    }

    /// Swaps two child windows.
    pub fn swap(&mut self, win1: &Rc<RefCell<MuttWindow>>, win2: &Rc<RefCell<MuttWindow>>) -> bool {
        let pos1 = self.children.iter().position(|c| Rc::ptr_eq(c, win1));
        let pos2 = self.children.iter().position(|c| Rc::ptr_eq(c, win2));

        if let (Some(p1), Some(p2)) = (pos1, pos2) {
            self.children.swap(p1, p2);
            self.actions |= WindowActionFlags::REFLOW;
            true
        } else {
            false
        }
    }

    /// Marks all windows in the tree for repaint.
    pub fn invalidate_all(win: &Rc<RefCell<Self>>) {
        win.borrow_mut().actions |= WindowActionFlags::REPAINT | WindowActionFlags::RECALC;
        let borrowed = win.borrow();
        for child in &borrowed.children {
            Self::invalidate_all(child);
        }
    }

    /// Returns the name of this window type for debugging.
    pub fn name(&self) -> &'static str {
        self.window_type.name()
    }

    /// Checks if two windows are the same.
    pub fn same_window(a: &Rc<RefCell<Self>>, b: &Rc<RefCell<Self>>) -> bool {
        Rc::ptr_eq(a, b)
    }

    /// Moves the cursor within this window.
    pub fn move_cursor(&self, out: &mut dyn Write, row: i16, col: i16) -> Result<()> {
        let abs_row = self.state.row_offset + row;
        let abs_col = self.state.col_offset + col;
        if abs_row < 0 || abs_col < 0 {
            return Ok(());
        }
        out.execute(MoveTo(abs_col as u16, abs_row as u16))?;
        Ok(())
    }

    /// Writes a character at the current cursor position.
    pub fn addch(&self, out: &mut dyn Write, ch: char) -> Result<()> {
        out.execute(Print(ch))?;
        Ok(())
    }

    /// Writes a string at the current cursor position.
    pub fn addstr(&self, out: &mut dyn Write, s: &str) -> Result<()> {
        out.execute(Print(s))?;
        Ok(())
    }

    /// Writes at most n characters of a string.
    pub fn addnstr(&self, out: &mut dyn Write, s: &str, n: usize) -> Result<()> {
        let truncated: String = s.chars().take(n).collect();
        out.execute(Print(truncated))?;
        Ok(())
    }

    /// Writes a formatted string at the current cursor position.
    pub fn printf(&self, out: &mut dyn Write, args: std::fmt::Arguments<'_>) -> Result<()> {
        out.execute(Print(args.to_string()))?;
        Ok(())
    }

    /// Clears the entire window.
    pub fn clear(&self, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
        if self.state.rows <= 0 {
            return Ok(());
        }
        for row in 0..self.state.rows {
            self.clearline(ctx, out, row)?;
        }
        Ok(())
    }

    /// Clears a single row within the window.
    pub fn clearline(&self, ctx: &mut GuiContext, out: &mut dyn Write, row: i16) -> Result<()> {
        if self.state.cols <= 0 {
            return Ok(());
        }
        self.move_cursor(out, row, 0)?;
        let current = mutt_curses_current_color(ctx);
        mutt_curses_set_color(ctx, out, &current)?;
        let spaces = " ".repeat(self.state.cols as usize);
        out.execute(Print(spaces))?;
        Ok(())
    }

    /// Clears from cursor to end of line.
    pub fn clrtoeol(&self, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
        let current = mutt_curses_current_color(ctx);
        mutt_curses_set_color(ctx, out, &current)?;
        out.execute(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::UntilNewLine,
        ))?;
        Ok(())
    }

    /// Gets the cursor position within the window.
    pub fn get_coords(&self) -> Result<(i16, i16)> {
        let (abs_col, abs_row) = cursor::position()?;
        let rel_row = abs_row as i16 - self.state.row_offset;
        let rel_col = abs_col as i16 - self.state.col_offset;
        Ok((rel_row, rel_col))
    }

    /// Calculates the wrap column for a given width and wrap setting.
    pub fn wrap_cols(width: i16, wrap: i16) -> i16 {
        if wrap < 0 {
            if width > -wrap {
                width + wrap
            } else {
                width
            }
        } else if wrap > 0 {
            if wrap < width {
                wrap
            } else {
                width
            }
        } else {
            width
        }
    }

    /// Flushes output to the terminal.
    pub fn flush(out: &mut dyn Write) -> Result<()> {
        out.flush()
    }

    /// Redraws the window tree (reflow, recalc, repaint).
    pub fn redraw(
        win: &Rc<RefCell<Self>>,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        // First pass: reflow if needed
        if win.borrow().actions.contains(WindowActionFlags::REFLOW) {
            crate::window_reflow(win);
        }

        Self::notify_all(win);

        // Second pass: recalc if needed
        Self::recalc_tree(win);

        // Third pass: repaint if needed
        Self::repaint_tree(win, ctx, out)?;

        Self::recursor(win, ctx, out)?;
        out.flush()?;
        Ok(())
    }

    /// Recursively recalculates windows that need it.
    fn recalc_tree(win: &Rc<RefCell<Self>>) {
        let mut borrowed = win.borrow_mut();

        if borrowed.actions.contains(WindowActionFlags::RECALC) {
            if let Some(recalc) = borrowed.recalc {
                recalc(&mut borrowed);
            }
            borrowed.actions.remove(WindowActionFlags::RECALC);
        }

        let children = borrowed.children.to_vec();
        drop(borrowed);

        for child in children {
            Self::recalc_tree(&child);
        }
    }

    /// Recursively repaints windows that need it.
    fn repaint_tree(
        win: &Rc<RefCell<Self>>,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        let mut borrowed = win.borrow_mut();

        if !borrowed.state.visible {
            return Ok(());
        }

        if borrowed.actions.contains(WindowActionFlags::REPAINT) {
            if let Some(repaint) = borrowed.repaint {
                repaint(&mut borrowed);
            }
            if let Some(draw) = borrowed.draw {
                draw(&mut borrowed, ctx, out)?;
            }
            borrowed.actions.remove(WindowActionFlags::REPAINT);
        }

        let children = borrowed.children.to_vec();
        drop(borrowed);

        for child in children {
            Self::repaint_tree(&child, ctx, out)?;
        }

        Ok(())
    }

    fn recursor(win: &Rc<RefCell<Self>>, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
        let root = Self::get_root(win);
        let focused = Self::get_focus(&root);
        let mut borrowed = focused.borrow_mut();
        let handled = if let Some(recursor) = borrowed.recursor {
            recursor(&mut borrowed, ctx, out)
        } else {
            false
        };
        drop(borrowed);

        if !handled {
            out.execute(Hide)?;
        }
        Ok(())
    }
}

/// Organize a panel so its status bar is on top or bottom.
///
/// Returns true if the order changed.
pub fn window_status_on_top(panel: &Rc<RefCell<MuttWindow>>, status_on_top: bool) -> bool {
    let mut borrowed = panel.borrow_mut();
    let status_pos = borrowed
        .children
        .iter()
        .position(|child| child.borrow().window_type == WindowType::StatusBar);
    let status_pos = match status_pos {
        Some(pos) => pos,
        None => return false,
    };

    let target_pos = if status_on_top {
        0
    } else {
        borrowed.children.len().saturating_sub(1)
    };

    if status_pos == target_pos {
        return false;
    }

    let status = borrowed.children.remove(status_pos);
    let insert_pos = if status_on_top {
        0
    } else {
        borrowed.children.len()
    };
    borrowed.children.insert(insert_pos, status);
    borrowed.actions |= WindowActionFlags::REFLOW;
    drop(borrowed);

    window_reflow(panel);
    let root = MuttWindow::get_root(panel);
    MuttWindow::invalidate_all(&root);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_type_name() {
        assert_eq!(WindowType::Root.name(), "Root");
        assert_eq!(WindowType::DlgIndex.name(), "DlgIndex");
    }

    #[test]
    fn window_type_is_dialog() {
        assert!(!WindowType::Root.is_dialog());
        assert!(!WindowType::Container.is_dialog());
        assert!(WindowType::DlgHelp.is_dialog());
        assert!(WindowType::DlgIndex.is_dialog());
        assert!(WindowType::DlgCompose.is_dialog());
    }

    #[test]
    fn window_state_new() {
        let state = WindowState::new(80, 24);
        assert_eq!(state.cols, 80);
        assert_eq!(state.rows, 24);
        assert!(state.visible);
        assert_eq!(state.col_offset, 0);
        assert_eq!(state.row_offset, 0);
    }

    #[test]
    fn window_add_child() {
        let parent = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&parent, Rc::clone(&child));

        assert_eq!(parent.borrow().children.len(), 1);
        assert!(child.borrow().parent.is_some());
    }

    #[test]
    fn window_remove_child() {
        let parent = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&parent, Rc::clone(&child));
        assert_eq!(parent.borrow().children.len(), 1);

        let removed = parent.borrow_mut().remove_child(&child);
        assert!(removed.is_some());
        assert_eq!(parent.borrow().children.len(), 0);
        assert!(child.borrow().parent.is_none());
    }

    #[test]
    fn window_find_child() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let message = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );

        MuttWindow::add_child(&root, Rc::clone(&container));
        MuttWindow::add_child(&container, Rc::clone(&message));

        let found = MuttWindow::find_child(&root, WindowType::Message);
        assert!(found.is_some());
        assert!(MuttWindow::same_window(&found.unwrap(), &message));

        let not_found = MuttWindow::find_child(&root, WindowType::Sidebar);
        assert!(not_found.is_none());
    }

    #[test]
    fn window_find_parent() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let message = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );

        MuttWindow::add_child(&root, Rc::clone(&container));
        MuttWindow::add_child(&container, Rc::clone(&message));

        let found = MuttWindow::find_parent(&message, WindowType::Root);
        assert!(found.is_some());
        assert!(MuttWindow::same_window(&found.unwrap(), &root));
    }

    #[test]
    fn window_status_on_top_reorders() {
        let panel = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            5,
        );
        panel.borrow_mut().state.cols = 80;
        panel.borrow_mut().state.rows = 5;

        let menu = MuttWindow::new(
            WindowType::Menu,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let sbar = MuttWindow::new(
            WindowType::StatusBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            1,
        );

        MuttWindow::add_child(&panel, Rc::clone(&menu));
        MuttWindow::add_child(&panel, Rc::clone(&sbar));

        assert!(window_status_on_top(&panel, true));
        assert!(MuttWindow::same_window(&panel.borrow().children[0], &sbar));

        assert!(window_status_on_top(&panel, false));
        let last_index = panel.borrow().children.len() - 1;
        assert!(MuttWindow::same_window(
            &panel.borrow().children[last_index],
            &sbar
        ));
    }

    #[test]
    fn window_visibility() {
        let parent = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&parent, Rc::clone(&child));

        assert!(child.borrow().is_visible());

        parent.borrow_mut().set_visible(false);
        assert!(!child.borrow().is_visible());
    }

    #[test]
    fn window_swap_children() {
        let parent = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child1 = MuttWindow::new(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );
        let child2 = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );

        MuttWindow::add_child(&parent, Rc::clone(&child1));
        MuttWindow::add_child(&parent, Rc::clone(&child2));

        assert!(MuttWindow::same_window(
            &parent.borrow().children[0],
            &child1
        ));
        assert!(MuttWindow::same_window(
            &parent.borrow().children[1],
            &child2
        ));

        let result = parent.borrow_mut().swap(&child1, &child2);
        assert!(result);

        assert!(MuttWindow::same_window(
            &parent.borrow().children[0],
            &child2
        ));
        assert!(MuttWindow::same_window(
            &parent.borrow().children[1],
            &child1
        ));
    }

    #[test]
    fn wrap_cols_negative() {
        assert_eq!(MuttWindow::wrap_cols(10, -5), 5);
        assert_eq!(MuttWindow::wrap_cols(4, -10), 4);
    }

    #[test]
    fn wrap_cols_zero() {
        assert_eq!(MuttWindow::wrap_cols(10, 0), 10);
    }

    #[test]
    fn wrap_cols_above_width() {
        assert_eq!(MuttWindow::wrap_cols(10, 12), 10);
    }

    #[test]
    fn remove_child_nonexistent() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        let removed = root.borrow_mut().remove_child(&child);
        assert!(removed.is_none());
        assert!(root.borrow().children.is_empty());
    }

    #[test]
    fn find_child_deep_nesting() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let mut current = Rc::clone(&root);
        for _ in 0..6 {
            let child = MuttWindow::new(
                WindowType::Container,
                WindowOrientation::Vertical,
                WindowSize::Maximise,
                0,
                0,
            );
            MuttWindow::add_child(&current, Rc::clone(&child));
            current = child;
        }
        let target = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );
        MuttWindow::add_child(&current, Rc::clone(&target));

        let found = MuttWindow::find_child(&root, WindowType::Message);
        assert!(found.is_some());
        assert!(MuttWindow::same_window(&found.unwrap(), &target));
    }

    #[test]
    fn find_parent_deep_nesting() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let mut current = Rc::clone(&root);
        for _ in 0..6 {
            let child = MuttWindow::new(
                WindowType::Container,
                WindowOrientation::Vertical,
                WindowSize::Maximise,
                0,
                0,
            );
            MuttWindow::add_child(&current, Rc::clone(&child));
            current = child;
        }
        let leaf = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );
        MuttWindow::add_child(&current, Rc::clone(&leaf));

        let found = MuttWindow::find_parent(&leaf, WindowType::Root);
        assert!(found.is_some());
        assert!(MuttWindow::same_window(&found.unwrap(), &root));
    }

    #[test]
    fn same_window_with_cloned_rc() {
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let cloned = Rc::clone(&win);
        assert!(MuttWindow::same_window(&win, &cloned));
    }
}
