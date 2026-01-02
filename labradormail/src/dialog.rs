//! Dialog stack management.
//!
//! Dialogs are pushed onto a stack and only the topmost dialog is visible.
//! This module provides functions to push, pop, and find dialogs.

use std::cell::RefCell;
use std::rc::Rc;

use crate::window::MuttWindow;
use crate::window::NotifyWindow;
use crate::window::WindowNotifyFlags;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowType;
use crate::window_reflow;

/// All dialogs container window.
///
/// This is a container that holds all dialog windows in a stack.
/// Only the topmost dialog is visible at any time.
pub struct AllDialogsWindow {
    window: Rc<RefCell<MuttWindow>>,
}

impl AllDialogsWindow {
    /// Creates a new all dialogs container window.
    pub fn new() -> Self {
        let window = MuttWindow::new(
            WindowType::AllDialogs,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        Self { window }
    }

    /// Returns a reference to the underlying window.
    pub fn window(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.window
    }

    /// Gets the top (visible) dialog.
    pub fn top(&self) -> Option<Rc<RefCell<MuttWindow>>> {
        let borrowed = self.window.borrow();
        borrowed.children.last().cloned()
    }

    /// Adds a dialog to the stack and makes it visible.
    pub fn push(&self, dialog: Rc<RefCell<MuttWindow>>) {
        dialog_stack_push(&self.window, dialog, true);
    }

    /// Removes the top dialog from the stack.
    pub fn pop(&self) -> Option<Rc<RefCell<MuttWindow>>> {
        dialog_stack_pop(&self.window, true)
    }

    /// Returns the number of dialogs in the stack.
    pub fn len(&self) -> usize {
        self.window.borrow().children.len()
    }

    /// Returns true if there are no dialogs.
    pub fn is_empty(&self) -> bool {
        self.window.borrow().children.is_empty()
    }
}

pub(crate) fn dialog_stack_push(
    container: &Rc<RefCell<MuttWindow>>,
    dialog: Rc<RefCell<MuttWindow>>,
    hide_current_top: bool,
) {
    if hide_current_top {
        let current_top = {
            let borrowed = container.borrow();
            borrowed.children.last().cloned()
        };
        if let Some(current_top) = current_top {
            current_top.borrow_mut().set_visible(false);
        }
    }

    MuttWindow::add_child(container, dialog);
    let top = {
        let borrowed = container.borrow();
        borrowed.children.last().cloned()
    };
    if let Some(top) = top {
        top.borrow_mut().set_visible(true);
        MuttWindow::notify_with_flags_rc(&top, NotifyWindow::Dialog, WindowNotifyFlags::VISIBLE);
    }

    window_reflow(container);
}

pub(crate) fn dialog_stack_pop(
    container: &Rc<RefCell<MuttWindow>>,
    reset_popped: bool,
) -> Option<Rc<RefCell<MuttWindow>>> {
    let popped = {
        let mut borrowed = container.borrow_mut();
        if borrowed.children.is_empty() {
            return None;
        }
        borrowed.children.pop()
    };

    if reset_popped {
        if let Some(ref dialog) = popped {
            MuttWindow::notify_with_flags_rc(
                dialog,
                NotifyWindow::Dialog,
                WindowNotifyFlags::HIDDEN,
            );
            dialog.borrow_mut().set_visible(false);
            dialog.borrow_mut().parent = None;
        }
    }

    let new_top = {
        let borrowed = container.borrow();
        borrowed.children.last().cloned()
    };
    if let Some(new_top) = new_top {
        new_top.borrow_mut().set_visible(true);
    } else {
        container.borrow_mut().focus = None;
    }

    window_reflow(container);
    popped
}

impl Default for AllDialogsWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Dialog window wrapper.
///
/// Provides convenience methods for working with dialog windows.
pub struct Dialog {
    window: Rc<RefCell<MuttWindow>>,
}

impl Dialog {
    /// Creates a new dialog window.
    pub fn new(window_type: WindowType) -> Self {
        assert!(window_type.is_dialog(), "Window type must be a dialog type");

        let window = MuttWindow::new(
            window_type,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        Self { window }
    }

    /// Returns a reference to the underlying window.
    pub fn window(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.window
    }

    /// Finds the parent dialog of a window.
    pub fn find(win: &Rc<RefCell<MuttWindow>>) -> Option<Rc<RefCell<MuttWindow>>> {
        // Check if this window is a dialog
        if win.borrow().window_type.is_dialog() {
            return Some(Rc::clone(win));
        }

        // Search up the tree
        let borrowed = win.borrow();
        if let Some(ref parent_weak) = borrowed.parent {
            if let Some(parent) = parent_weak.upgrade() {
                drop(borrowed);
                return Dialog::find(&parent);
            }
        }

        None
    }

    /// Adds a child window to this dialog.
    pub fn add_child(&self, child: Rc<RefCell<MuttWindow>>) {
        MuttWindow::add_child(&self.window, child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static OBSERVED: Mutex<Vec<(NotifyWindow, WindowNotifyFlags)>> = Mutex::new(Vec::new());

    fn record_observer(notify_type: NotifyWindow, event: &crate::window::EventWindow) {
        OBSERVED
            .lock()
            .expect("observer lock")
            .push((notify_type, event.flags));
    }

    #[test]
    fn all_dialogs_push_pop() {
        let all_dialogs = AllDialogsWindow::new();
        all_dialogs.window.borrow_mut().state.cols = 80;
        all_dialogs.window.borrow_mut().state.rows = 22;

        assert!(all_dialogs.is_empty());

        let dialog1 = Dialog::new(WindowType::DlgIndex);
        all_dialogs.push(Rc::clone(dialog1.window()));

        assert_eq!(all_dialogs.len(), 1);
        assert!(dialog1.window().borrow().state.visible);

        let dialog2 = Dialog::new(WindowType::DlgCompose);
        all_dialogs.push(Rc::clone(dialog2.window()));

        assert_eq!(all_dialogs.len(), 2);
        assert!(!dialog1.window().borrow().state.visible);
        assert!(dialog2.window().borrow().state.visible);

        let popped = all_dialogs.pop();
        assert!(popped.is_some());
        assert_eq!(all_dialogs.len(), 1);
        assert!(dialog1.window().borrow().state.visible);
    }

    #[test]
    fn dialog_find() {
        let dialog = Dialog::new(WindowType::DlgIndex);
        let container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let child = MuttWindow::new(
            WindowType::Index,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(dialog.window(), Rc::clone(&container));
        MuttWindow::add_child(&container, Rc::clone(&child));

        let found = Dialog::find(&child);
        assert!(found.is_some());
        assert!(MuttWindow::same_window(&found.unwrap(), dialog.window()));
    }

    #[test]
    #[should_panic(expected = "Window type must be a dialog type")]
    fn dialog_requires_dialog_type() {
        Dialog::new(WindowType::Container);
    }

    #[test]
    fn pop_empty_returns_none() {
        let all_dialogs = AllDialogsWindow::new();
        let popped = all_dialogs.pop();
        assert!(popped.is_none());
    }

    #[test]
    fn push_pop_visibility_and_notifications() {
        let all_dialogs = AllDialogsWindow::new();
        OBSERVED.lock().expect("observer lock").clear();

        let dialog = Dialog::new(WindowType::DlgIndex);
        dialog.window().borrow_mut().add_observer(record_observer);
        all_dialogs.push(Rc::clone(dialog.window()));

        let observed = OBSERVED.lock().expect("observer lock");
        assert!(observed.iter().any(|(ty, flags)| {
            *ty == NotifyWindow::Dialog && flags.contains(WindowNotifyFlags::VISIBLE)
        }));
        drop(observed);

        let popped = all_dialogs.pop();
        assert!(popped.is_some());
        assert!(!dialog.window().borrow().state.visible);
    }

    #[test]
    fn dialog_find_returns_none_when_missing() {
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
        MuttWindow::add_child(&root, Rc::clone(&child));

        let found = Dialog::find(&child);
        assert!(found.is_none());
    }

    #[test]
    fn all_dialogs_top_and_len() {
        let all_dialogs = AllDialogsWindow::new();
        assert!(all_dialogs.top().is_none());
        assert_eq!(all_dialogs.len(), 0);

        let dialog = Dialog::new(WindowType::DlgHelp);
        all_dialogs.push(Rc::clone(dialog.window()));
        assert!(all_dialogs.top().is_some());
        assert_eq!(all_dialogs.len(), 1);
    }
}
