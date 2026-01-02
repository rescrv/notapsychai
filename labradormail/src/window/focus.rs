//! Focus management for window trees.

use std::cell::RefCell;
use std::rc::Rc;

use super::MuttWindow;
use super::NotifyWindow;
use super::WindowActionFlags;
use super::WindowNotifyFlags;
use super::WindowType;

impl MuttWindow {
    /// Searches the subtree for a window with pending_focus set.
    pub(crate) fn find_pending_focus(win: &Rc<RefCell<Self>>) -> Option<Rc<RefCell<MuttWindow>>> {
        if win.borrow().pending_focus {
            return Some(Rc::clone(win));
        }
        for child in &win.borrow().children {
            if let Some(found) = Self::find_pending_focus(child) {
                return Some(found);
            }
        }
        None
    }

    /// Gets the currently focused window by following the focus chain.
    ///
    /// Starting from this window, follows the focus pointer down to find
    /// the leaf window that has focus.
    pub fn get_focus(win: &Rc<RefCell<Self>>) -> Rc<RefCell<MuttWindow>> {
        let borrowed = win.borrow();
        if let Some(ref focus_weak) = borrowed.focus {
            if let Some(focus) = focus_weak.upgrade() {
                drop(borrowed);
                return Self::get_focus(&focus);
            }
        }
        Rc::clone(win)
    }

    /// Sets the focus to the given window.
    ///
    /// Updates the focus chain from the window up to the root.
    /// Returns the previously focused window, if any.
    ///
    /// If called on a window not yet in the tree (no parent), marks the window
    /// with `pending_focus` so that focus will be established when it's added.
    pub fn set_focus(win: &Rc<RefCell<Self>>) -> Option<Rc<RefCell<MuttWindow>>> {
        let has_parent = win.borrow().parent.is_some();

        if !has_parent {
            // Window is not in tree yet. Mark it for focus when added.
            let mut borrowed = win.borrow_mut();
            borrowed.pending_focus = true;
            borrowed.focus = None;
            return None;
        }

        // Clear pending_focus since we're establishing focus now
        win.borrow_mut().pending_focus = false;

        let root = Self::get_root(win);
        let old_focus = Self::get_focus(&root);

        // Set focus chain from win up to root
        let mut current = Rc::clone(win);
        loop {
            let parent_weak = {
                let borrowed = current.borrow();
                borrowed.parent.clone()
            };

            if let Some(parent_weak) = parent_weak {
                if let Some(parent) = parent_weak.upgrade() {
                    parent.borrow_mut().focus = Some(Rc::downgrade(&current));
                    current = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Clear focus below the new focused window
        win.borrow_mut().focus = None;

        if Rc::ptr_eq(&old_focus, win) {
            None
        } else {
            Self::notify_with_flags_rc(win, NotifyWindow::Focus, WindowNotifyFlags::empty());
            if let Some(help_bar) = Self::find_child(&root, WindowType::HelpBar) {
                help_bar.borrow_mut().actions |=
                    WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
            }
            Some(old_focus)
        }
    }

    /// Checks if this window currently has focus.
    pub fn is_focused(win: &Rc<RefCell<Self>>) -> bool {
        let root = Self::get_root(win);
        let focused = Self::get_focus(&root);
        Rc::ptr_eq(&focused, win)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::MuttWindow;
    use crate::window::WindowOrientation;
    use crate::window::WindowSize;
    use crate::window::WindowType;

    #[test]
    fn window_focus_chain() {
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
            WindowSize::Fixed,
            10,
            10,
        );
        let leaf = MuttWindow::new(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            5,
            5,
        );

        MuttWindow::add_child(&root, Rc::clone(&container));
        MuttWindow::add_child(&container, Rc::clone(&leaf));

        // Set focus to leaf
        MuttWindow::set_focus(&leaf);

        // Check focus chain
        assert!(MuttWindow::is_focused(&leaf));
        assert!(!MuttWindow::is_focused(&container));
        assert!(!MuttWindow::is_focused(&root));

        // get_focus from root should return leaf
        let focused = MuttWindow::get_focus(&root);
        assert!(MuttWindow::same_window(&focused, &leaf));
    }

    #[test]
    fn window_focus_change() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child1 = MuttWindow::new(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let child2 = MuttWindow::new(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );

        MuttWindow::add_child(&root, Rc::clone(&child1));
        MuttWindow::add_child(&root, Rc::clone(&child2));

        // Set focus to child1
        MuttWindow::set_focus(&child1);
        assert!(MuttWindow::is_focused(&child1));
        assert!(!MuttWindow::is_focused(&child2));

        // Change focus to child2
        let old_focus = MuttWindow::set_focus(&child2);
        assert!(old_focus.is_some());
        assert!(MuttWindow::same_window(&old_focus.unwrap(), &child1));
        assert!(!MuttWindow::is_focused(&child1));
        assert!(MuttWindow::is_focused(&child2));
    }

    #[test]
    fn window_focus_updates_help_bar() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let helpbar = MuttWindow::new(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            1,
        );
        let child = MuttWindow::new(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );

        MuttWindow::add_child(&root, Rc::clone(&helpbar));
        MuttWindow::add_child(&root, Rc::clone(&child));

        MuttWindow::set_focus(&child);

        let helpbar_actions = helpbar.borrow().actions;
        assert!(helpbar_actions.contains(WindowActionFlags::RECALC));
        assert!(helpbar_actions.contains(WindowActionFlags::REPAINT));
    }

    #[test]
    fn window_remove_clears_focus() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let child = MuttWindow::new(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );

        MuttWindow::add_child(&root, Rc::clone(&child));
        MuttWindow::set_focus(&child);

        // Remove child - focus should be cleared
        root.borrow_mut().remove_child(&child);
        let focused = MuttWindow::get_focus(&root);
        assert!(MuttWindow::same_window(&focused, &root));
    }

    #[test]
    fn set_focus_before_add_child_establishes_focus() {
        // This tests the scenario where set_focus is called on an orphan window,
        // which should set pending_focus and later establish focus when added to tree.
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let dialog = MuttWindow::new(
            WindowType::DlgHelp,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            20,
            10,
        );

        // Set focus on orphan dialog (not yet in tree)
        MuttWindow::set_focus(&dialog);

        // Verify pending_focus is set
        assert!(dialog.borrow().pending_focus);

        // Add dialog and ensure focus is established
        MuttWindow::add_child(&root, Rc::clone(&dialog));

        // pending_focus should be cleared
        assert!(!dialog.borrow().pending_focus);

        // Focus should now be established - dialog should be focused
        let focused = MuttWindow::get_focus(&root);
        assert!(MuttWindow::same_window(&focused, &dialog));
    }

    #[test]
    fn get_focus_when_no_focus_set_returns_root() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let focused = MuttWindow::get_focus(&root);
        assert!(MuttWindow::same_window(&focused, &root));
    }

    #[test]
    fn focus_with_orphaned_window_returns_root() {
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
            WindowSize::Fixed,
            10,
            10,
        );
        MuttWindow::add_child(&root, Rc::clone(&child));
        MuttWindow::set_focus(&child);

        root.borrow_mut().remove_child(&child);
        let focused = MuttWindow::get_focus(&root);
        assert!(MuttWindow::same_window(&focused, &root));
    }

    #[test]
    fn set_focus_on_root_marks_pending() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let old = MuttWindow::set_focus(&root);
        assert!(old.is_none());
        assert!(root.borrow().pending_focus);
    }
}
