//! Window reflow algorithm for automatic layout.
//!
//! This module implements the three-pass layout algorithm that distributes
//! space among windows based on their sizing mode (Fixed, Maximise, Minimise).

use std::cell::RefCell;
use std::rc::Rc;

use crate::window::MuttWindow;
use crate::window::WindowActionFlags;
use crate::window::WindowOrientation;
use crate::window::WindowSize;

/// Main reflow function - delegates to horiz/vert based on orientation.
pub fn window_reflow(win: &Rc<RefCell<MuttWindow>>) {
    let mut borrowed = win.borrow_mut();
    borrowed.actions.remove(WindowActionFlags::REFLOW);

    if !borrowed.state.visible {
        return;
    }

    let orient = borrowed.orient;
    drop(borrowed);

    match orient {
        WindowOrientation::Vertical => window_reflow_axis(win, Axis::Vertical),
        WindowOrientation::Horizontal => window_reflow_axis(win, Axis::Horizontal),
    }
}

/// Axis abstraction for unified reflow logic.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    /// Gets the primary dimension (cols for horizontal, rows for vertical).
    fn get_primary(&self, win: &MuttWindow) -> i16 {
        match self {
            Axis::Horizontal => win.state.cols,
            Axis::Vertical => win.state.rows,
        }
    }

    /// Sets the primary dimension.
    fn set_primary(&self, win: &mut MuttWindow, value: i16) {
        match self {
            Axis::Horizontal => win.state.cols = value,
            Axis::Vertical => win.state.rows = value,
        }
    }

    /// Gets the secondary dimension (rows for horizontal, cols for vertical).
    fn get_secondary(&self, win: &MuttWindow) -> i16 {
        match self {
            Axis::Horizontal => win.state.rows,
            Axis::Vertical => win.state.cols,
        }
    }

    /// Sets the secondary dimension.
    fn set_secondary(&self, win: &mut MuttWindow, value: i16) {
        match self {
            Axis::Horizontal => win.state.rows = value,
            Axis::Vertical => win.state.cols = value,
        }
    }

    /// Gets the requested primary dimension.
    fn get_req_primary(&self, win: &MuttWindow) -> i16 {
        match self {
            Axis::Horizontal => win.req_cols,
            Axis::Vertical => win.req_rows,
        }
    }

    /// Gets the primary offset.
    fn get_primary_offset(&self, win: &MuttWindow) -> i16 {
        match self {
            Axis::Horizontal => win.state.col_offset,
            Axis::Vertical => win.state.row_offset,
        }
    }

    /// Sets the primary offset.
    fn set_primary_offset(&self, win: &mut MuttWindow, value: i16) {
        match self {
            Axis::Horizontal => win.state.col_offset = value,
            Axis::Vertical => win.state.row_offset = value,
        }
    }

    /// Gets the secondary offset.
    fn get_secondary_offset(&self, win: &MuttWindow) -> i16 {
        match self {
            Axis::Horizontal => win.state.row_offset,
            Axis::Vertical => win.state.col_offset,
        }
    }

    /// Sets the secondary offset.
    fn set_secondary_offset(&self, win: &mut MuttWindow, value: i16) {
        match self {
            Axis::Horizontal => win.state.row_offset = value,
            Axis::Vertical => win.state.col_offset = value,
        }
    }
}

/// Reflow children along the specified axis.
///
/// Three-pass algorithm:
/// 1. Pass 1 - Minimal allocation: Give Fixed windows their requested size,
///    Maximise windows 1 unit, Minimise windows full parent size then recursively reflow
/// 2. Pass 2 - Sharing: Distribute remaining space evenly among Maximise windows
/// 3. Pass 3 - Position: Calculate absolute offsets and recurse into children
///
/// After layout, if this window is MINIMISE-sized, shrink to fit children (NeoMutt behavior).
fn window_reflow_axis(win: &Rc<RefCell<MuttWindow>>, axis: Axis) {
    let borrowed = win.borrow();
    let parent_primary = axis.get_primary(&borrowed).max(0);
    let parent_secondary = axis.get_secondary(&borrowed).max(0);
    let parent_primary_offset = axis.get_primary_offset(&borrowed);
    let parent_secondary_offset = axis.get_secondary_offset(&borrowed);
    let parent_size = borrowed.size;

    // Collect visible children
    let children: Vec<_> = borrowed
        .children
        .iter()
        .filter(|c| c.borrow().state.visible)
        .cloned()
        .collect();
    drop(borrowed);

    if children.is_empty() {
        // MINIMISE with no visible children shrinks to zero
        if parent_size == WindowSize::Minimise {
            let mut borrowed = win.borrow_mut();
            axis.set_primary(&mut borrowed, 0);
        }
        return;
    }

    // Pass 1: Minimal allocation
    let mut used_primary: i16 = 0;
    let mut maximise_count: i16 = 0;

    for child in &children {
        let mut child_borrowed = child.borrow_mut();
        axis.set_secondary(&mut child_borrowed, parent_secondary);

        match child_borrowed.size {
            WindowSize::Fixed => {
                let req = axis.get_req_primary(&child_borrowed);
                let available = (parent_primary - used_primary).max(0);
                let allocated = req.min(available).max(0);
                axis.set_primary(&mut child_borrowed, allocated);
                used_primary += allocated;
            }
            WindowSize::Maximise => {
                let allocated = if parent_primary > used_primary { 1 } else { 0 };
                axis.set_primary(&mut child_borrowed, allocated);
                used_primary += allocated;
                if allocated > 0 {
                    maximise_count += 1;
                }
            }
            WindowSize::Minimise => {
                // Give it the full remaining space, then reflow to find actual size
                let remaining = (parent_primary - used_primary).max(0);
                axis.set_primary(&mut child_borrowed, remaining);
                drop(child_borrowed);
                window_reflow(child);
                let child_borrowed = child.borrow();
                used_primary += axis.get_primary(&child_borrowed).max(0);
            }
        }
    }

    // Pass 2: Distribute remaining space to Maximise windows
    // NeoMutt rounding: give extra to earlier windows first
    let remaining = parent_primary - used_primary;
    if remaining > 0 && maximise_count > 0 {
        let extra_per_window = remaining / maximise_count;
        let mut leftover = remaining % maximise_count;

        for child in &children {
            let mut child_borrowed = child.borrow_mut();
            if child_borrowed.size == WindowSize::Maximise {
                let current = axis.get_primary(&child_borrowed);
                axis.set_primary(&mut child_borrowed, current + extra_per_window);
                if leftover > 0 {
                    let current = axis.get_primary(&child_borrowed);
                    axis.set_primary(&mut child_borrowed, current + 1);
                    leftover -= 1;
                }
            }
        }
    }

    // Pass 3: Calculate positions and recurse
    let mut current_primary = parent_primary_offset;
    let mut total_child_primary: i16 = 0;
    for child in &children {
        let mut child_borrowed = child.borrow_mut();
        axis.set_primary_offset(&mut child_borrowed, current_primary);
        axis.set_secondary_offset(&mut child_borrowed, parent_secondary_offset);

        // Mark for recalc/repaint if size or position changed
        if child_borrowed.state.cols != child_borrowed.old.cols
            || child_borrowed.state.rows != child_borrowed.old.rows
            || child_borrowed.state.col_offset != child_borrowed.old.col_offset
            || child_borrowed.state.row_offset != child_borrowed.old.row_offset
        {
            child_borrowed.actions |= WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
        }

        let child_primary = axis.get_primary(&child_borrowed);
        current_primary += child_primary;
        total_child_primary += child_primary;
        drop(child_borrowed);

        // Recurse into children (unless already reflowed for Minimise)
        let child_borrowed = child.borrow();
        if child_borrowed.size != WindowSize::Minimise {
            drop(child_borrowed);
            window_reflow(child);
        }
    }

    // Second pass for MINIMISE children to pick up final offsets
    for child in &children {
        if child.borrow().size == WindowSize::Minimise {
            window_reflow(child);
        }
    }

    // MINIMISE: shrink parent to fit children
    if parent_size == WindowSize::Minimise {
        let mut borrowed = win.borrow_mut();
        axis.set_primary(&mut borrowed, total_child_primary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::WindowType;

    #[test]
    fn reflow_vertical_fixed() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let help = MuttWindow::new(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );
        let message = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );
        let content = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&help));
        MuttWindow::add_child(&root, Rc::clone(&content));
        MuttWindow::add_child(&root, Rc::clone(&message));

        window_reflow(&root);

        assert_eq!(help.borrow().state.rows, 1, "helpbar should be 1 row");
        assert_eq!(help.borrow().state.row_offset, 0, "helpbar at top");

        assert_eq!(message.borrow().state.rows, 1, "message should be 1 row");
        assert_eq!(message.borrow().state.row_offset, 23, "message at bottom");

        assert_eq!(
            content.borrow().state.rows,
            22,
            "content gets remaining 22 rows"
        );
        assert_eq!(
            content.borrow().state.row_offset,
            1,
            "content starts at row 1"
        );
    }

    #[test]
    fn reflow_horizontal_fixed() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Horizontal,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let sidebar = MuttWindow::new(
            WindowType::Sidebar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            20,
            24,
        );
        let content = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&sidebar));
        MuttWindow::add_child(&root, Rc::clone(&content));

        window_reflow(&root);

        assert_eq!(sidebar.borrow().state.cols, 20, "sidebar should be 20 cols");
        assert_eq!(sidebar.borrow().state.col_offset, 0, "sidebar at left");

        assert_eq!(
            content.borrow().state.cols,
            60,
            "content gets remaining 60 cols"
        );
        assert_eq!(
            content.borrow().state.col_offset,
            20,
            "content starts at col 20"
        );
    }

    #[test]
    fn reflow_multiple_maximise() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let panel1 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let panel2 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&panel1));
        MuttWindow::add_child(&root, Rc::clone(&panel2));

        window_reflow(&root);

        assert_eq!(panel1.borrow().state.rows, 12, "panel1 gets half");
        assert_eq!(panel2.borrow().state.rows, 12, "panel2 gets half");
    }

    #[test]
    fn reflow_invisible_children() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let visible = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let invisible = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        invisible.borrow_mut().state.visible = false;

        MuttWindow::add_child(&root, Rc::clone(&visible));
        MuttWindow::add_child(&root, Rc::clone(&invisible));

        window_reflow(&root);

        assert_eq!(visible.borrow().state.rows, 24, "visible gets all space");
    }

    #[test]
    fn reflow_nested() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Horizontal,
            WindowSize::Maximise,
            0,
            0,
        );
        let sidebar = MuttWindow::new(
            WindowType::Sidebar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            20,
            0,
        );
        let content = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&container));
        MuttWindow::add_child(&container, Rc::clone(&sidebar));
        MuttWindow::add_child(&container, Rc::clone(&content));

        window_reflow(&root);

        assert_eq!(container.borrow().state.rows, 24, "container fills root");
        assert_eq!(sidebar.borrow().state.cols, 20, "sidebar is 20 cols");
        assert_eq!(sidebar.borrow().state.rows, 24, "sidebar gets full height");
        assert_eq!(
            content.borrow().state.cols,
            60,
            "content gets remaining width"
        );
    }

    #[test]
    fn reflow_minimise_shrink_vertical() {
        // Parent is MINIMISE with Fixed children - should shrink to fit
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let minimise_container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Minimise,
            0,
            0,
        );

        let child1 = MuttWindow::new(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            2,
        );

        let child2 = MuttWindow::new(
            WindowType::StatusBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            3,
        );

        MuttWindow::add_child(&root, Rc::clone(&minimise_container));
        MuttWindow::add_child(&minimise_container, Rc::clone(&child1));
        MuttWindow::add_child(&minimise_container, Rc::clone(&child2));

        window_reflow(&root);

        // Container should shrink to 2 + 3 = 5 rows
        assert_eq!(
            minimise_container.borrow().state.rows,
            5,
            "MINIMISE container shrinks to fit children"
        );
        assert_eq!(child1.borrow().state.rows, 2, "child1 keeps 2 rows");
        assert_eq!(child2.borrow().state.rows, 3, "child2 keeps 3 rows");
    }

    #[test]
    fn reflow_fixed_overflow_clamps() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            10,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 10;

        let child1 = MuttWindow::new(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            8,
        );
        let child2 = MuttWindow::new(
            WindowType::StatusBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            8,
        );
        let child3 = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            8,
        );

        MuttWindow::add_child(&root, Rc::clone(&child1));
        MuttWindow::add_child(&root, Rc::clone(&child2));
        MuttWindow::add_child(&root, Rc::clone(&child3));

        window_reflow(&root);

        assert_eq!(child1.borrow().state.rows, 8, "first child gets requested");
        assert_eq!(child2.borrow().state.rows, 2, "second child is clamped");
        assert_eq!(child3.borrow().state.rows, 0, "third child gets no space");
    }

    #[test]
    fn reflow_negative_parent_size() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            0,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = -5;

        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&child));

        window_reflow(&root);

        assert_eq!(child.borrow().state.rows, 0, "negative parent clamps to 0");
    }

    #[test]
    fn reflow_minimise_child_offsets_update() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            10,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 10;

        let header = MuttWindow::new(
            WindowType::StatusBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            2,
        );
        let minimise = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Minimise,
            0,
            0,
        );
        let inner = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            1,
        );

        MuttWindow::add_child(&minimise, Rc::clone(&inner));
        MuttWindow::add_child(&root, Rc::clone(&header));
        MuttWindow::add_child(&root, Rc::clone(&minimise));

        window_reflow(&root);

        assert_eq!(minimise.borrow().state.row_offset, 2);
        assert_eq!(inner.borrow().state.row_offset, 2);
    }

    #[test]
    fn reflow_minimise_shrink_horizontal() {
        // Parent is MINIMISE with Fixed children horizontally - should shrink to fit
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Horizontal,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let minimise_container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Horizontal,
            WindowSize::Minimise,
            0,
            0,
        );

        let child1 = MuttWindow::new(
            WindowType::Sidebar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            24,
        );

        let child2 = MuttWindow::new(
            WindowType::Sidebar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            15,
            24,
        );

        MuttWindow::add_child(&root, Rc::clone(&minimise_container));
        MuttWindow::add_child(&minimise_container, Rc::clone(&child1));
        MuttWindow::add_child(&minimise_container, Rc::clone(&child2));

        window_reflow(&root);

        // Container should shrink to 10 + 15 = 25 cols
        assert_eq!(
            minimise_container.borrow().state.cols,
            25,
            "MINIMISE container shrinks to fit children horizontally"
        );
        assert_eq!(child1.borrow().state.cols, 10, "child1 keeps 10 cols");
        assert_eq!(child2.borrow().state.cols, 15, "child2 keeps 15 cols");
    }

    #[test]
    fn reflow_minimise_empty() {
        // MINIMISE with no visible children should shrink to zero
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let minimise_container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Minimise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&minimise_container));

        window_reflow(&root);

        assert_eq!(
            minimise_container.borrow().state.rows,
            0,
            "MINIMISE with no children shrinks to 0"
        );
    }

    #[test]
    fn reflow_sharing_rounding_odd() {
        // With odd space to share, earlier windows should get extra
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            25, // Odd number - can't split evenly among 2
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 25;

        let panel1 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let panel2 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&panel1));
        MuttWindow::add_child(&root, Rc::clone(&panel2));

        window_reflow(&root);

        // 25 rows total, 2 MAXIMISE windows
        // Each gets 1 initially, 23 remaining
        // 23 / 2 = 11 each, with 1 leftover to first
        // panel1 = 1 + 11 + 1 = 13, panel2 = 1 + 11 = 12
        assert_eq!(
            panel1.borrow().state.rows,
            13,
            "first MAXIMISE gets extra from rounding"
        );
        assert_eq!(panel2.borrow().state.rows, 12, "second MAXIMISE gets less");
    }

    #[test]
    fn reflow_sharing_rounding_three_windows() {
        // Three MAXIMISE windows with space that doesn't divide evenly
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            26, // 26 rows among 3 windows
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 26;

        let panel1 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let panel2 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let panel3 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&panel1));
        MuttWindow::add_child(&root, Rc::clone(&panel2));
        MuttWindow::add_child(&root, Rc::clone(&panel3));

        window_reflow(&root);

        // 26 rows total, 3 MAXIMISE windows
        // Each gets 1 initially, 23 remaining
        // 23 / 3 = 7 each, with 2 leftover distributed to first two
        // panel1 = 1 + 7 + 1 = 9, panel2 = 1 + 7 + 1 = 9, panel3 = 1 + 7 = 8
        assert_eq!(
            panel1.borrow().state.rows,
            9,
            "first MAXIMISE gets extra from rounding"
        );
        assert_eq!(
            panel2.borrow().state.rows,
            9,
            "second MAXIMISE gets extra from rounding"
        );
        assert_eq!(
            panel3.borrow().state.rows,
            8,
            "third MAXIMISE gets no extra"
        );
    }

    #[test]
    fn reflow_zero_width_parent() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Horizontal,
            WindowSize::Fixed,
            0,
            5,
        );
        root.borrow_mut().state.cols = 0;
        root.borrow_mut().state.rows = 5;

        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        MuttWindow::add_child(&root, Rc::clone(&child));

        window_reflow(&root);

        assert!(child.borrow().state.cols >= 0);
    }

    #[test]
    fn reflow_zero_height_parent() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            0,
        );
        root.borrow_mut().state.cols = 10;
        root.borrow_mut().state.rows = 0;

        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        MuttWindow::add_child(&root, Rc::clone(&child));

        window_reflow(&root);

        assert!(child.borrow().state.rows >= 0);
    }

    #[test]
    fn reflow_minimise_with_only_invisible_children() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Minimise,
            10,
            10,
        );
        root.borrow_mut().state.cols = 10;
        root.borrow_mut().state.rows = 10;

        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            2,
        );
        child.borrow_mut().state.visible = false;
        MuttWindow::add_child(&root, Rc::clone(&child));

        window_reflow(&root);

        assert_eq!(root.borrow().state.rows, 0);
    }

    #[test]
    fn reflow_minimise_then_maximise() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            20,
            10,
        );
        root.borrow_mut().state.cols = 20;
        root.borrow_mut().state.rows = 10;

        let minimise = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Minimise,
            0,
            0,
        );
        let minimise_child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            2,
        );
        MuttWindow::add_child(&minimise, Rc::clone(&minimise_child));

        let maximise = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&minimise));
        MuttWindow::add_child(&root, Rc::clone(&maximise));

        window_reflow(&root);

        assert_eq!(minimise.borrow().state.rows, 2);
        assert_eq!(maximise.borrow().state.rows, 8);
    }

    #[test]
    fn reflow_horizontal_with_minimise_child() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Horizontal,
            WindowSize::Fixed,
            20,
            5,
        );
        root.borrow_mut().state.cols = 20;
        root.borrow_mut().state.rows = 5;

        let minimise = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Horizontal,
            WindowSize::Minimise,
            0,
            0,
        );
        let minimise_child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            5,
            0,
        );
        MuttWindow::add_child(&minimise, Rc::clone(&minimise_child));

        let maximise = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&minimise));
        MuttWindow::add_child(&root, Rc::clone(&maximise));

        window_reflow(&root);

        assert_eq!(minimise.borrow().state.cols, 5);
        assert_eq!(maximise.borrow().state.cols, 15);
    }

    #[test]
    fn reflow_deep_nesting() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        root.borrow_mut().state.cols = 80;
        root.borrow_mut().state.rows = 24;

        let mut current = Rc::clone(&root);
        for _ in 0..12 {
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

        window_reflow(&root);

        assert_eq!(current.borrow().state.rows, 24);
        assert_eq!(current.borrow().state.cols, 80);
    }

    #[test]
    fn reflow_negative_remaining_space_clamps() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Horizontal,
            WindowSize::Fixed,
            5,
            5,
        );
        root.borrow_mut().state.cols = 5;
        root.borrow_mut().state.rows = 5;

        let child1 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            4,
            0,
        );
        let child2 = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            4,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&child1));
        MuttWindow::add_child(&root, Rc::clone(&child2));

        window_reflow(&root);

        let total = child1.borrow().state.cols + child2.borrow().state.cols;
        assert!(child1.borrow().state.cols >= 0);
        assert!(child2.borrow().state.cols >= 0);
        assert!(total <= root.borrow().state.cols);
    }
}
