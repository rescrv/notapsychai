//! Core window management types and implementation.
//!
//! This module provides the fundamental window structures including `MuttWindow`,
//! window types, orientations, sizing modes, and notification systems.

use super::LayoutBuilder;
use super::NotifyWindow;
use super::WindowNotifyFlags;
use super::WindowObserver;
use super::WindowWidget;
use std::collections::HashMap;
use std::io::Result;
use std::io::Write;

use bitflags::bitflags;
use crossterm::cursor;
use crossterm::cursor::Hide;
use crossterm::cursor::SetCursorStyle;
use crossterm::cursor::Show;
use crossterm::ExecutableCommand;

use crate::context::GuiContext;
use crate::geom::Point;
use crate::geom::Rect;
use crate::geom::Size;
use crate::global::Bindings;
use crate::help_data::HelpData;
use crate::reflow::window_reflow;
use crate::render::move_cursor;

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
    /// Fixed size (uses requested size).
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
    /// Help dialog.
    DlgHelp,
    /// Index dialog.
    DlgIndex,
    /// Agent chat dialog.
    DlgAgent,

    // Common windows
    /// Custom window.
    Custom,
    /// Help bar.
    HelpBar,
    /// Index window (message list).
    Index,
    /// Message window.
    Message,
    /// Pager window (message view).
    Pager,
    /// Sidebar window.
    Sidebar,
    /// Status bar.
    StatusBar,
}

impl WindowType {
    /// Returns the name of this window type for debugging.
    pub fn name(&self) -> &'static str {
        match self {
            WindowType::Root => "Root",
            WindowType::Container => "Container",
            WindowType::AllDialogs => "AllDialogs",
            WindowType::DlgHelp => "DlgHelp",
            WindowType::DlgIndex => "DlgIndex",
            WindowType::DlgAgent => "DlgAgent",
            WindowType::Custom => "Custom",
            WindowType::HelpBar => "HelpBar",
            WindowType::Index => "Index",
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
            WindowType::DlgHelp | WindowType::DlgIndex | WindowType::DlgAgent
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
    /// Window rectangle in absolute screen coordinates.
    pub rect: Rect,
}

impl WindowState {
    /// Creates a new window state with the given dimensions.
    pub fn new(size: Size) -> Self {
        Self {
            visible: true,
            rect: Rect::new(Point::new(0, 0), size),
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

impl CursorState {
    /// Sets the cursor visibility state.
    ///
    /// Matches NeoMutt's cursor semantics:
    /// - Invisible: cursor is hidden
    /// - Visible: cursor is shown (default line cursor)
    /// - VeryVisible: cursor is shown as a blinking block
    pub fn set(self, out: &mut dyn Write) -> Result<()> {
        match self {
            CursorState::Invisible => {
                out.execute(Hide)?;
            }
            CursorState::Visible => {
                out.execute(Show)?;
                out.execute(SetCursorStyle::DefaultUserShape)?;
            }
            CursorState::VeryVisible => {
                out.execute(Show)?;
                if out.execute(SetCursorStyle::BlinkingBlock).is_err() {
                    out.execute(SetCursorStyle::SteadyBlock)?;
                }
            }
        }
        Ok(())
    }
}

/// Cursor positioning behavior after rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorBehavior {
    /// Hide the cursor.
    Hidden,
    /// Move cursor to a position within the window.
    Positioned {
        row: i16,
        col: i16,
        state: CursorState,
    },
}

/// Rendering mode for widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Paint and return cursor behavior.
    Paint,
    /// Only return cursor behavior without drawing.
    CursorOnly,
}

/// Window tree identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub(crate) u64);

/// Main window structure.
///
/// Represents a window in the hierarchical window tree. Windows can contain
/// child windows and are laid out according to their orientation and size mode.
pub struct Window {
    /// Requested size.
    pub req_size: Size,
    /// Current window state.
    pub state: WindowState,
    /// Previous window state (for notifications).
    pub old: WindowState,
    /// Layout direction for children.
    pub orient: WindowOrientation,
    /// Sizing mode.
    pub size: WindowSize,
    /// Pending actions.
    pub(crate) actions: WindowActionFlags,

    /// Parent window.
    pub parent: Option<WindowId>,
    /// Child windows.
    pub children: Vec<WindowId>,
    /// Currently focused child.
    pub focus: Option<WindowId>,

    /// Window type identifier.
    pub window_type: WindowType,
    /// Widget data for this window.
    pub widget: Option<WindowWidget>,

    /// Help bar data for this window (optional).
    pub help_data: Option<HelpData>,

    /// Key bindings active for this window scope.
    pub bindings: Bindings,

    /// Registered observers for window events.
    pub observers: Vec<WindowObserver>,

    /// Set when `set_focus` is called on an orphan window (no parent).
    /// Cleared when the focus chain is properly established after being added to tree.
    pub(crate) pending_focus: bool,
}

impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("req_size", &self.req_size)
            .field("state", &self.state)
            .field("orient", &self.orient)
            .field("size", &self.size)
            .field("window_type", &self.window_type)
            .field("children_count", &self.children.len())
            .finish()
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        )
    }
}

impl Window {
    /// Creates a new window instance.
    pub fn new(
        window_type: WindowType,
        orient: WindowOrientation,
        size: WindowSize,
        cols: i16,
        rows: i16,
    ) -> Self {
        let req_size = Size::new(cols, rows);
        Self {
            req_size,
            state: WindowState::new(req_size),
            old: WindowState::default(),
            orient,
            size,
            actions: WindowActionFlags::RECALC | WindowActionFlags::REPAINT,
            parent: None,
            children: Vec::new(),
            focus: None,
            window_type,
            widget: None,
            help_data: None,
            bindings: Bindings::new(),
            observers: Vec::new(),
            pending_focus: false,
        }
    }

    /// Stores a widget implementation.
    pub fn set_widget(&mut self, widget: WindowWidget) {
        self.widget = Some(widget);
    }

    /// Sets the requested size and marks for reflow if changed.
    pub fn set_req_size(&mut self, cols: i16, rows: i16) {
        let new_size = Size::new(cols, rows);
        if self.req_size != new_size {
            self.req_size = new_size;
            self.mark_reflow();
        }
    }

    /// Sets the window orientation and marks for reflow if changed.
    pub fn set_orient(&mut self, orient: WindowOrientation) {
        if self.orient != orient {
            self.orient = orient;
            self.mark_reflow();
        }
    }

    /// Sets the window sizing mode and marks for reflow if changed.
    pub fn set_sizing_mode(&mut self, size: WindowSize) {
        if self.size != size {
            self.size = size;
            self.mark_reflow();
        }
    }

    /// Marks this window for reflow.
    pub fn mark_reflow(&mut self) {
        self.actions.insert(WindowActionFlags::REFLOW);
    }

    /// Marks this window for recalc.
    pub fn mark_recalc(&mut self) {
        self.actions.insert(WindowActionFlags::RECALC);
    }

    /// Marks this window for repaint.
    pub fn mark_repaint(&mut self) {
        self.actions.insert(WindowActionFlags::REPAINT);
    }

    /// Marks this window for recalc and repaint.
    pub fn mark_recalc_repaint(&mut self) {
        self.actions
            .insert(WindowActionFlags::RECALC | WindowActionFlags::REPAINT);
    }

    /// Returns true if any window action is pending.
    pub fn is_dirty(&self) -> bool {
        self.actions.intersects(
            WindowActionFlags::REFLOW | WindowActionFlags::RECALC | WindowActionFlags::REPAINT,
        )
    }

    /// Returns the raw action flags.
    pub fn action_flags(&self) -> WindowActionFlags {
        self.actions
    }

    /// Clears all action flags.
    pub fn clear_actions(&mut self) {
        self.actions = WindowActionFlags::empty();
    }

    /// Returns true if a reflow is pending.
    pub fn needs_reflow(&self) -> bool {
        self.actions.contains(WindowActionFlags::REFLOW)
    }

    /// Clears the reflow action.
    pub fn clear_reflow(&mut self) {
        self.actions.remove(WindowActionFlags::REFLOW);
    }

    /// Returns true if recalc was pending and clears it.
    pub fn take_recalc(&mut self) -> bool {
        let has_recalc = self.actions.contains(WindowActionFlags::RECALC);
        if has_recalc {
            self.actions.remove(WindowActionFlags::RECALC);
        }
        has_recalc
    }

    /// Returns true if repaint was pending and clears it.
    pub fn take_repaint(&mut self) -> bool {
        let has_repaint = self.actions.contains(WindowActionFlags::REPAINT);
        if has_repaint {
            self.actions.remove(WindowActionFlags::REPAINT);
        }
        has_repaint
    }

    /// Returns a reference to the widget, if present.
    pub fn widget_ref(&self) -> Option<&WindowWidget> {
        self.widget.as_ref()
    }

    /// Returns a mutable reference to the widget, if present.
    pub fn widget_mut(&mut self) -> Option<&mut WindowWidget> {
        self.widget.as_mut()
    }

    /// Returns the name of this window type for debugging.
    pub fn name(&self) -> &'static str {
        self.window_type.name()
    }

    /// Gets the cursor position within the window.
    pub fn get_coords(&self) -> Result<(i16, i16)> {
        let (abs_col, abs_row) = cursor::position()?;
        let rel_row = abs_row as i16 - self.state.rect.origin.row;
        let rel_col = abs_col as i16 - self.state.rect.origin.col;
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
}

/// Window hierarchy container for explicit context-based access.
#[derive(Default)]
pub struct WindowTree {
    windows: HashMap<WindowId, Window>,
    next_id: u64,
}

impl WindowTree {
    /// Creates a new, empty window tree.
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            next_id: 0,
        }
    }

    /// Creates a layout builder for attaching children under `parent`.
    pub fn layout(&mut self, parent: WindowId) -> LayoutBuilder<'_> {
        LayoutBuilder::new(self, parent)
    }

    /// Adds a new window to the tree and returns its ID.
    pub fn add_window(
        &mut self,
        window_type: WindowType,
        orient: WindowOrientation,
        size: WindowSize,
        cols: i16,
        rows: i16,
    ) -> WindowId {
        let win = Window::new(window_type, orient, size, cols, rows);
        let id = WindowId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.windows.insert(id, win);
        id
    }

    /// Adds a dialog window with default dialog sizing and returns its ID.
    pub fn add_dialog(&mut self, window_type: WindowType) -> WindowId {
        assert!(window_type.is_dialog(), "Window type must be a dialog type");
        self.add_window(
            window_type,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        )
    }

    /// Returns a reference to a window.
    pub fn get(&self, id: WindowId) -> &Window {
        self.windows
            .get(&id)
            .unwrap_or_else(|| panic!("invalid window id: {:?}", id))
    }

    /// Returns a mutable reference to a window.
    pub fn get_mut(&mut self, id: WindowId) -> &mut Window {
        self.windows
            .get_mut(&id)
            .unwrap_or_else(|| panic!("invalid window id: {:?}", id))
    }

    fn contains_window(&self, root: WindowId, target: WindowId) -> bool {
        if root == target {
            return true;
        }
        for child in self.get(root).children.clone() {
            if self.contains_window(child, target) {
                return true;
            }
        }
        false
    }

    /// Adds a child window.
    pub fn add_child(&mut self, parent: WindowId, child: WindowId) {
        assert!(parent != child, "cannot add window as its own child");
        assert!(
            self.get(child).parent.is_none(),
            "cannot add child: window already has a parent"
        );
        assert!(
            !self.contains_window(parent, child),
            "cannot add child: window already exists in parent subtree"
        );
        assert!(
            !self.contains_window(child, parent),
            "cannot add child: would create cycle"
        );
        self.get_mut(child).parent = Some(parent);
        {
            let parent_ref = self.get_mut(parent);
            parent_ref.children.push(child);
            parent_ref.mark_reflow();
        }
        self.notify_with_flags(child, NotifyWindow::Add, WindowNotifyFlags::empty());

        if let Some(focus_target) = self.find_pending_focus(child) {
            self.set_focus(focus_target);
        }
    }

    /// Removes a child window from this window.
    ///
    /// Also clears focus if the removed window or any of its descendants had focus.
    pub fn remove_child(&mut self, parent: WindowId, child: WindowId) -> Option<WindowId> {
        let pos = self.get(parent).children.iter().position(|c| *c == child)?;
        let removed = self.get_mut(parent).children.remove(pos);

        if self.get(parent).focus == Some(child) {
            self.get_mut(parent).focus = None;
        }

        self.notify_with_flags(removed, NotifyWindow::Delete, WindowNotifyFlags::empty());
        self.get_mut(removed).parent = None;
        self.get_mut(parent).mark_reflow();
        Some(removed)
    }

    /// Checks if this window is visible, including all ancestors.
    pub fn is_visible(&self, win: WindowId) -> bool {
        if !self.get(win).state.visible {
            return false;
        }
        if let Some(parent) = self.get(win).parent {
            return self.is_visible(parent);
        }
        true
    }

    /// Checks if this window was visible, based on old state and ancestors.
    pub fn was_visible(&self, win: WindowId) -> bool {
        if !self.get(win).old.visible {
            return false;
        }
        if let Some(parent) = self.get(win).parent {
            return self.was_visible(parent);
        }
        true
    }

    /// Sets the visibility of this window.
    ///
    /// Note: This only sets the visibility flag. The parent window's reflow
    /// flag should be set separately if needed.
    pub fn set_visible(&mut self, win: WindowId, visible: bool) {
        if self.get(win).state.visible == visible {
            return;
        }
        let win_ref = self.get_mut(win);
        win_ref.state.visible = visible;
        win_ref.mark_reflow();
    }

    /// Sets the visibility of a window and marks parent for reflow.
    pub fn set_visible_with_parent(&mut self, win: WindowId, visible: bool) {
        if self.get(win).state.visible == visible {
            return;
        }
        let parent = self.get(win).parent;
        {
            let win_ref = self.get_mut(win);
            win_ref.state.visible = visible;
            win_ref.mark_reflow();
        }
        if let Some(parent) = parent {
            self.get_mut(parent).mark_reflow();
        }
    }

    /// Finds a child window by type (depth-first search).
    pub fn find_child(&self, win: WindowId, window_type: WindowType) -> Option<WindowId> {
        for child in self.get(win).children.iter().copied() {
            if self.get(child).window_type == window_type {
                return Some(child);
            }
            if let Some(found) = self.find_child(child, window_type) {
                return Some(found);
            }
        }
        None
    }

    /// Finds a parent window by type.
    pub fn find_parent(&self, win: WindowId, window_type: WindowType) -> Option<WindowId> {
        let parent = self.get(win).parent?;
        if self.get(parent).window_type == window_type {
            return Some(parent);
        }
        self.find_parent(parent, window_type)
    }

    /// Gets the root window of the tree containing this window.
    pub fn get_root(&self, win: WindowId) -> WindowId {
        if let Some(parent) = self.get(win).parent {
            return self.get_root(parent);
        }
        win
    }

    /// Swaps two child windows.
    pub fn swap(&mut self, parent: WindowId, win1: WindowId, win2: WindowId) -> bool {
        let pos1 = self.get(parent).children.iter().position(|c| *c == win1);
        let pos2 = self.get(parent).children.iter().position(|c| *c == win2);

        if let (Some(p1), Some(p2)) = (pos1, pos2) {
            let children = &mut self.get_mut(parent).children;
            children.swap(p1, p2);
            self.get_mut(parent).mark_reflow();
            true
        } else {
            false
        }
    }

    /// Marks all windows in the tree for repaint.
    pub fn invalidate_all(&mut self, win: WindowId) {
        {
            let win_ref = self.get_mut(win);
            win_ref.mark_recalc_repaint();
        }
        let children = self.get(win).children.clone();
        for child in children {
            self.invalidate_all(child);
        }
    }

    /// Checks if two windows are the same.
    pub fn same_window(&self, a: WindowId, b: WindowId) -> bool {
        a == b
    }

    /// Pushes a window onto a container stack.
    /// Hides the current top window (if any) and shows the new one.
    pub fn stack_push(&mut self, container: WindowId, window: WindowId) {
        if let Some(top) = self.stack_top(container) {
            self.set_visible(top, false);
        }

        self.add_child(container, window);
        self.set_visible(window, true);
        window_reflow(self, container);
    }

    /// Brings a window to the top of a container stack.
    ///
    /// If the window is already a child, it is reordered to the top.
    /// Otherwise it is added as a new child.
    pub fn stack_bring_to_top(&mut self, container: WindowId, window: WindowId) {
        if let Some(top) = self.stack_top(container) {
            if top != window {
                self.set_visible(top, false);
            }
        }

        let pos = self
            .get(container)
            .children
            .iter()
            .position(|c| *c == window);
        if let Some(pos) = pos {
            let child = self.get_mut(container).children.remove(pos);
            self.get_mut(container).children.push(child);
        } else {
            self.add_child(container, window);
        }

        self.set_visible(window, true);
        window_reflow(self, container);
    }

    /// Pops the top window from a container stack.
    /// Returns the popped window. Shows the new top window (if any).
    pub fn stack_pop(&mut self, container: WindowId) -> Option<WindowId> {
        let popped = {
            let children = &mut self.get_mut(container).children;
            if children.is_empty() {
                return None;
            }
            children.pop()
        };

        if let Some(popped) = popped {
            self.set_visible(popped, false);
            self.get_mut(popped).parent = None;
        }

        if let Some(new_top) = self.stack_top(container) {
            self.set_visible(new_top, true);
        } else {
            self.get_mut(container).focus = None;
        }

        window_reflow(self, container);
        popped
    }

    /// Demotes the top window to the bottom of the stack without removing it.
    ///
    /// Returns the demoted window, if any.
    pub fn stack_demote_top(&mut self, container: WindowId) -> Option<WindowId> {
        let len = self.get(container).children.len();
        if len <= 1 {
            return None;
        }

        let top = {
            let children = &mut self.get_mut(container).children;
            children.pop()
        }?;
        self.set_visible(top, false);
        self.get_mut(container).children.insert(0, top);

        if let Some(new_top) = self.stack_top(container) {
            self.set_visible(new_top, true);
            self.set_focus(new_top);
        } else {
            self.get_mut(container).focus = None;
        }

        window_reflow(self, container);
        Some(top)
    }

    /// Returns the top window of a container stack.
    pub fn stack_top(&self, container: WindowId) -> Option<WindowId> {
        self.get(container).children.last().copied()
    }

    /// Returns the number of windows in the stack.
    pub fn stack_len(&self, container: WindowId) -> usize {
        self.get(container).children.len()
    }

    /// Hides the top window of a container stack.
    pub fn stack_hide_top(&mut self, container: WindowId) {
        if let Some(top) = self.stack_top(container) {
            self.set_visible_with_parent(top, false);
        }
        window_reflow(self, container);
    }

    /// Shows the top window of a container stack.
    pub fn stack_show_top(&mut self, container: WindowId) {
        if let Some(top) = self.stack_top(container) {
            self.set_visible_with_parent(top, true);
        }
        window_reflow(self, container);
    }

    /// Redraws the window tree (reflow, recalc, repaint).
    pub fn redraw(
        &mut self,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        if self.get(win).needs_reflow() {
            window_reflow(self, win);
        }

        self.notify_all(win);

        self.update_tree(win);
        let focus = self.get_focus(self.get_root(win));
        let mut cursor_behavior = None;
        self.render_tree(win, ctx, out, focus, &mut cursor_behavior)?;
        if cursor_behavior.is_none() {
            cursor_behavior = self.render_cursor_only(focus, ctx, out)?;
        }
        self.apply_cursor_behavior(focus, cursor_behavior, ctx, out)?;
        out.flush()?;
        Ok(())
    }

    fn update_tree(&mut self, win: WindowId) {
        let (children, has_recalc) = {
            let win_ref = self.get_mut(win);
            let has_recalc = win_ref.take_recalc();
            (win_ref.children.clone(), has_recalc)
        };

        if has_recalc {
            if let Some(mut widget) = self.get_mut(win).widget.take() {
                widget.update(self, win);
                self.get_mut(win).widget = Some(widget);
            }
        }

        for child in children {
            self.update_tree(child);
        }
    }

    fn render_tree(
        &mut self,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        focus: WindowId,
        cursor_behavior: &mut Option<CursorBehavior>,
    ) -> Result<()> {
        let (children, is_visible, has_repaint) = {
            let win_ref = self.get_mut(win);
            let is_visible = win_ref.state.visible;
            let has_repaint = is_visible && win_ref.take_repaint();
            (win_ref.children.clone(), is_visible, has_repaint)
        };

        if !is_visible {
            return Ok(());
        }

        if has_repaint {
            if let Some(mut widget) = self.get_mut(win).widget.take() {
                let result = widget.render(self, win, ctx, out, RenderMode::Paint);
                if win == focus {
                    *cursor_behavior = Some(result?);
                } else {
                    result?;
                }
                self.get_mut(win).widget = Some(widget);
            }
        }

        for child in children {
            self.render_tree(child, ctx, out, focus, cursor_behavior)?;
        }

        Ok(())
    }

    fn render_cursor_only(
        &mut self,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<Option<CursorBehavior>> {
        if let Some(mut widget) = self.get_mut(win).widget.take() {
            let result = widget.render(self, win, ctx, out, RenderMode::CursorOnly);
            self.get_mut(win).widget = Some(widget);
            return result.map(Some);
        }
        Ok(None)
    }

    fn apply_cursor_behavior(
        &mut self,
        win: WindowId,
        cursor_behavior: Option<CursorBehavior>,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        match cursor_behavior.unwrap_or(CursorBehavior::Hidden) {
            CursorBehavior::Hidden => {
                out.execute(Hide)?;
            }
            CursorBehavior::Positioned { row, col, state } => {
                if move_cursor(out, self.get(win), row, col).is_ok() {
                    let _ = ctx.set_cursor(out, state);
                } else {
                    out.execute(Hide)?;
                }
            }
        }
        Ok(())
    }
}

/// Organize a panel so its status bar is on top or bottom.
///
/// Returns true if the order changed.
pub fn window_status_on_top(tree: &mut WindowTree, panel: WindowId, status_on_top: bool) -> bool {
    let status_pos = tree
        .get(panel)
        .children
        .iter()
        .position(|child| tree.get(*child).window_type == WindowType::StatusBar);
    let status_pos = match status_pos {
        Some(pos) => pos,
        None => return false,
    };

    let target_pos = if status_on_top {
        0
    } else {
        tree.get(panel).children.len().saturating_sub(1)
    };

    if status_pos == target_pos {
        return false;
    }

    let status = tree.get_mut(panel).children.remove(status_pos);
    let insert_pos = if status_on_top {
        0
    } else {
        tree.get(panel).children.len()
    };
    tree.get_mut(panel).children.insert(insert_pos, status);
    tree.get_mut(panel).mark_reflow();

    window_reflow(tree, panel);
    let root = tree.get_root(panel);
    tree.invalidate_all(root);
    true
}
