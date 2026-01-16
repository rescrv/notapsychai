//! Window reflow algorithm for automatic layout.
//!
//! This module implements the three-pass layout algorithm that distributes
//! space among windows based on their sizing mode (Fixed, Maximise, Minimise).

use crate::window::Window;
use crate::window::WindowId;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowTree;

/// Main reflow function - delegates to horiz/vert based on orientation.
pub fn window_reflow(tree: &mut WindowTree, win: WindowId) {
    {
        let win_ref = tree.get_mut(win);
        win_ref.clear_reflow();
        if !win_ref.state.visible {
            return;
        }
    }

    let orient = tree.get(win).orient;
    match orient {
        WindowOrientation::Vertical => window_reflow_axis(tree, win, Axis::Vertical),
        WindowOrientation::Horizontal => window_reflow_axis(tree, win, Axis::Horizontal),
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
    fn get_primary(&self, win: &Window) -> i16 {
        match self {
            Axis::Horizontal => win.state.rect.size.cols,
            Axis::Vertical => win.state.rect.size.rows,
        }
    }

    /// Sets the primary dimension.
    fn set_primary(&self, win: &mut Window, value: i16) {
        match self {
            Axis::Horizontal => win.state.rect.size.cols = value,
            Axis::Vertical => win.state.rect.size.rows = value,
        }
    }

    /// Gets the secondary dimension (rows for horizontal, cols for vertical).
    fn get_secondary(&self, win: &Window) -> i16 {
        match self {
            Axis::Horizontal => win.state.rect.size.rows,
            Axis::Vertical => win.state.rect.size.cols,
        }
    }

    /// Sets the secondary dimension.
    fn set_secondary(&self, win: &mut Window, value: i16) {
        match self {
            Axis::Horizontal => win.state.rect.size.rows = value,
            Axis::Vertical => win.state.rect.size.cols = value,
        }
    }

    /// Gets the requested primary dimension.
    fn get_req_primary(&self, win: &Window) -> i16 {
        match self {
            Axis::Horizontal => win.req_size.cols,
            Axis::Vertical => win.req_size.rows,
        }
    }

    /// Gets the primary offset.
    fn get_primary_offset(&self, win: &Window) -> i16 {
        match self {
            Axis::Horizontal => win.state.rect.origin.col,
            Axis::Vertical => win.state.rect.origin.row,
        }
    }

    /// Sets the primary offset.
    fn set_primary_offset(&self, win: &mut Window, value: i16) {
        match self {
            Axis::Horizontal => win.state.rect.origin.col = value,
            Axis::Vertical => win.state.rect.origin.row = value,
        }
    }

    /// Gets the secondary offset.
    fn get_secondary_offset(&self, win: &Window) -> i16 {
        match self {
            Axis::Horizontal => win.state.rect.origin.row,
            Axis::Vertical => win.state.rect.origin.col,
        }
    }

    /// Sets the secondary offset.
    fn set_secondary_offset(&self, win: &mut Window, value: i16) {
        match self {
            Axis::Horizontal => win.state.rect.origin.row = value,
            Axis::Vertical => win.state.rect.origin.col = value,
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
fn window_reflow_axis(tree: &mut WindowTree, win: WindowId, axis: Axis) {
    let parent_primary = axis.get_primary(tree.get(win)).max(0);
    let parent_secondary = axis.get_secondary(tree.get(win)).max(0);
    let parent_primary_offset = axis.get_primary_offset(tree.get(win));
    let parent_secondary_offset = axis.get_secondary_offset(tree.get(win));
    let parent_size = tree.get(win).size;

    let children: Vec<_> = tree
        .get(win)
        .children
        .iter()
        .copied()
        .filter(|c| tree.get(*c).state.visible)
        .collect();

    if children.is_empty() {
        if parent_size == WindowSize::Minimise {
            let win_ref = tree.get_mut(win);
            axis.set_primary(win_ref, 0);
        }
        return;
    }

    let mut used_primary: i16 = 0;
    let mut maximise_count: i16 = 0;

    for child in &children {
        let child_ref = tree.get_mut(*child);
        axis.set_secondary(child_ref, parent_secondary);

        match child_ref.size {
            WindowSize::Fixed => {
                let req = axis.get_req_primary(child_ref);
                let available = (parent_primary - used_primary).max(0);
                let allocated = req.min(available).max(0);
                axis.set_primary(child_ref, allocated);
                used_primary += allocated;
            }
            WindowSize::Maximise => {
                let allocated = if parent_primary > used_primary { 1 } else { 0 };
                axis.set_primary(child_ref, allocated);
                used_primary += allocated;
                if allocated > 0 {
                    maximise_count += 1;
                }
            }
            WindowSize::Minimise => {
                let remaining = (parent_primary - used_primary).max(0);
                axis.set_primary(child_ref, remaining);
                let _ = child_ref;
                window_reflow(tree, *child);
                let child_ref = tree.get(*child);
                used_primary += axis.get_primary(child_ref).max(0);
            }
        }
    }

    let remaining = parent_primary - used_primary;
    if remaining > 0 && maximise_count > 0 {
        let extra_per_window = remaining / maximise_count;
        let mut leftover = remaining % maximise_count;

        for child in &children {
            let child_ref = tree.get_mut(*child);
            if child_ref.size == WindowSize::Maximise {
                let current = axis.get_primary(child_ref);
                axis.set_primary(child_ref, current + extra_per_window);
                if leftover > 0 {
                    let current = axis.get_primary(child_ref);
                    axis.set_primary(child_ref, current + 1);
                    leftover -= 1;
                }
            }
        }
    }

    let mut current_primary = parent_primary_offset;
    let mut total_child_primary: i16 = 0;
    for child in &children {
        {
            let child_ref = tree.get_mut(*child);
            axis.set_primary_offset(child_ref, current_primary);
            axis.set_secondary_offset(child_ref, parent_secondary_offset);

            if child_ref.state.rect.size != child_ref.old.rect.size
                || child_ref.state.rect.origin != child_ref.old.rect.origin
            {
                child_ref.mark_recalc_repaint();
            }

            let child_primary = axis.get_primary(child_ref);
            current_primary += child_primary;
            total_child_primary += child_primary;
        }

        if tree.get(*child).size != WindowSize::Minimise {
            window_reflow(tree, *child);
        }
    }

    for child in &children {
        if tree.get(*child).size == WindowSize::Minimise {
            window_reflow(tree, *child);
        }
    }

    if parent_size == WindowSize::Minimise {
        let win_ref = tree.get_mut(win);
        axis.set_primary(win_ref, total_child_primary.max(0));
    }
}
