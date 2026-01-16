//! Focus management for window trees.

use super::NotifyWindow;
use super::WindowId;
use super::WindowNotifyFlags;
use super::WindowTree;
use super::WindowType;

impl WindowTree {
    /// Searches the subtree for a window with pending_focus set.
    pub(crate) fn find_pending_focus(&self, win: WindowId) -> Option<WindowId> {
        if self.get(win).pending_focus {
            return Some(win);
        }
        for child in self.get(win).children.iter().copied() {
            if let Some(found) = self.find_pending_focus(child) {
                return Some(found);
            }
        }
        None
    }

    /// Gets the currently focused window by following the focus chain.
    ///
    /// Starting from this window, follows the focus pointer down to find
    /// the leaf window that has focus.
    pub fn get_focus(&self, win: WindowId) -> WindowId {
        if let Some(focus) = self.get(win).focus {
            return self.get_focus(focus);
        }
        win
    }

    /// Sets the focus to the given window.
    ///
    /// Updates the focus chain from the window up to the root.
    /// Returns the previously focused window, if any.
    ///
    /// If called on a window not yet in the tree (no parent), marks the window
    /// with `pending_focus` so that focus will be established when it's added.
    pub fn set_focus(&mut self, win: WindowId) -> Option<WindowId> {
        let has_parent = self.get(win).parent.is_some();

        if !has_parent {
            let win_ref = self.get_mut(win);
            win_ref.pending_focus = true;
            win_ref.focus = None;
            return None;
        }

        self.get_mut(win).pending_focus = false;

        let root = self.get_root(win);
        let old_focus = self.get_focus(root);

        let mut current = win;
        loop {
            let parent = self.get(current).parent;

            if let Some(parent) = parent {
                self.get_mut(parent).focus = Some(current);
                current = parent;
            } else {
                break;
            }
        }

        self.get_mut(win).focus = None;

        if old_focus == win {
            None
        } else {
            self.notify_with_flags(win, NotifyWindow::Focus, WindowNotifyFlags::empty());
            // The help bar displays key bindings for the focused window, so it must
            // recalculate and repaint whenever focus changes.
            if let Some(help_bar) = self.find_child(root, WindowType::HelpBar) {
                self.get_mut(help_bar).mark_recalc_repaint();
            }
            Some(old_focus)
        }
    }

    /// Checks if this window currently has focus.
    pub fn is_focused(&self, win: WindowId) -> bool {
        let root = self.get_root(win);
        let focused = self.get_focus(root);
        focused == win
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::WindowActionFlags;
    use crate::window::WindowOrientation;
    use crate::window::WindowSize;
    use crate::window::WindowType;

    #[test]
    fn window_focus_chain() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let container = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let leaf = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            5,
            5,
        );

        tree.add_child(root, container);
        tree.add_child(container, leaf);

        tree.set_focus(leaf);

        assert!(tree.is_focused(leaf));
        assert!(!tree.is_focused(container));
        assert!(!tree.is_focused(root));

        let focused = tree.get_focus(root);
        assert!(tree.same_window(focused, leaf));
    }

    #[test]
    fn window_focus_change() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child1 = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let child2 = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );

        tree.add_child(root, child1);
        tree.add_child(root, child2);

        tree.set_focus(child1);
        assert!(tree.is_focused(child1));
        assert!(!tree.is_focused(child2));

        let old_focus = tree.set_focus(child2);
        assert!(old_focus.is_some());
        assert!(tree.same_window(old_focus.unwrap(), child1));
        assert!(!tree.is_focused(child1));
        assert!(tree.is_focused(child2));
    }

    #[test]
    fn window_focus_updates_help_bar() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let helpbar = tree.add_window(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            1,
        );
        let child = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );

        tree.add_child(root, helpbar);
        tree.add_child(root, child);

        tree.set_focus(child);

        let helpbar_actions = tree.get(helpbar).actions;
        assert!(helpbar_actions.contains(WindowActionFlags::RECALC));
        assert!(helpbar_actions.contains(WindowActionFlags::REPAINT));
    }

    #[test]
    fn window_remove_clears_focus() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );

        tree.add_child(root, child);
        tree.set_focus(child);

        tree.remove_child(root, child);
        let focused = tree.get_focus(root);
        assert!(tree.same_window(focused, root));
    }

    #[test]
    fn set_focus_before_add_child_establishes_focus() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let dialog = tree.add_window(
            WindowType::DlgHelp,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            20,
            10,
        );

        tree.set_focus(dialog);

        assert!(tree.get(dialog).pending_focus);

        tree.add_child(root, dialog);

        assert!(!tree.get(dialog).pending_focus);

        let focused = tree.get_focus(root);
        assert!(tree.same_window(focused, dialog));
    }

    #[test]
    fn get_focus_when_no_focus_set_returns_root() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let focused = tree.get_focus(root);
        assert!(tree.same_window(focused, root));
    }

    #[test]
    fn focus_with_orphaned_window_returns_root() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        tree.add_child(root, child);
        tree.set_focus(child);

        tree.remove_child(root, child);
        let focused = tree.get_focus(root);
        assert!(tree.same_window(focused, root));
    }

    #[test]
    fn set_focus_on_root_marks_pending() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let old = tree.set_focus(root);
        assert!(old.is_none());
        assert!(tree.get(root).pending_focus);
    }
}
