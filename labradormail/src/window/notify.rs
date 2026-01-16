//! Window observer and notification support.

use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use bitflags::bitflags;

use super::Window;
use super::WindowId;
use super::WindowTree;
use super::WindowWidget;

bitflags! {
    /// Window notification flags (for observers).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct WindowNotifyFlags: u8 {
        /// Window became taller.
        const TALLER = 1 << 0;
        /// Window became shorter.
        const SHORTER = 1 << 1;
        /// Window became wider.
        const WIDER = 1 << 2;
        /// Window became narrower.
        const NARROWER = 1 << 3;
        /// Window moved position.
        const MOVED = 1 << 4;
        /// Window became visible.
        const VISIBLE = 1 << 5;
        /// Window became hidden.
        const HIDDEN = 1 << 6;
    }
}

/// Window notification types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyWindow {
    /// New window added.
    Add,
    /// Window about to be deleted.
    Delete,
    /// State changed (size, visibility).
    State,
    /// New dialog created.
    Dialog,
    /// Focus changed.
    Focus,
}

/// Event data for window notifications.
#[derive(Debug, Clone, Copy)]
pub struct EventWindow {
    /// Window ID.
    pub win: WindowId,
    /// Notification flags.
    pub flags: WindowNotifyFlags,
}

/// Global counter for generating unique observer IDs.
static NEXT_OBSERVER_ID: AtomicU64 = AtomicU64::new(1);

/// Type alias for window observer callbacks.
pub type WindowObserverFn = fn(NotifyWindow, &EventWindow, &mut WindowTree);

/// Trait for window data that tracks its own observer ID.
///
/// Implementing this trait allows using the helper function
/// `handle_observer_delete` to clean up the observer on Delete notifications.
pub trait HasObserverId {
    /// Takes the stored observer ID, leaving None in its place.
    fn take_observer_id(&mut self) -> Option<u64>;
}

/// Handles the common Delete notification cleanup for window observers.
///
/// Extracts the observer ID from typed widget data and removes the observer.
/// Returns true if an observer was found and removed.
pub fn handle_observer_delete(tree: &mut WindowTree, win: WindowId) -> bool {
    let observer_id = {
        let win_ref = tree.get_mut(win);
        win_ref
            .widget_mut()
            .and_then(WindowWidget::take_observer_id)
    };
    if let Some(id) = observer_id {
        tree.get_mut(win).remove_observer(id);
        return true;
    }
    false
}

/// Standard observer that sets RECALC on State notification.
pub fn standard_observer_recalc(
    notify_type: NotifyWindow,
    event: &EventWindow,
    tree: &mut WindowTree,
) {
    match notify_type {
        NotifyWindow::State => {
            tree.get_mut(event.win).mark_recalc();
        }
        NotifyWindow::Delete => {
            handle_observer_delete(tree, event.win);
        }
        _ => {}
    }
}

/// A registered observer for window events.
#[derive(Clone)]
pub struct WindowObserver {
    /// Unique identifier for this observer.
    pub id: u64,
    /// The callback function to invoke.
    pub callback: WindowObserverFn,
}

impl std::fmt::Debug for WindowObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowObserver")
            .field("id", &self.id)
            .finish()
    }
}

impl WindowObserver {
    /// Creates a new observer with a unique ID.
    pub fn new(callback: WindowObserverFn) -> Self {
        Self {
            id: NEXT_OBSERVER_ID.fetch_add(1, Ordering::SeqCst),
            callback,
        }
    }
}

impl Window {
    /// Computes the notification flags by comparing old and current state.
    pub fn compute_notify_flags(&self) -> WindowNotifyFlags {
        self.compute_notify_flags_with_visibility(self.old.visible, self.state.visible)
    }

    pub(crate) fn compute_notify_flags_with_visibility(
        &self,
        was_visible: bool,
        is_visible: bool,
    ) -> WindowNotifyFlags {
        let mut flags = WindowNotifyFlags::empty();

        if self.state.rect.size.rows > self.old.rect.size.rows {
            flags |= WindowNotifyFlags::TALLER;
        } else if self.state.rect.size.rows < self.old.rect.size.rows {
            flags |= WindowNotifyFlags::SHORTER;
        }

        if self.state.rect.size.cols > self.old.rect.size.cols {
            flags |= WindowNotifyFlags::WIDER;
        } else if self.state.rect.size.cols < self.old.rect.size.cols {
            flags |= WindowNotifyFlags::NARROWER;
        }

        if self.state.rect.origin.row != self.old.rect.origin.row
            || self.state.rect.origin.col != self.old.rect.origin.col
        {
            flags |= WindowNotifyFlags::MOVED;
        }

        if is_visible && !was_visible {
            flags |= WindowNotifyFlags::VISIBLE;
        } else if !is_visible && was_visible {
            flags |= WindowNotifyFlags::HIDDEN;
        }

        flags
    }

    /// Adds an observer to this window.
    ///
    /// Returns the observer ID which can be used to remove it later.
    pub fn add_observer(&mut self, callback: WindowObserverFn) -> u64 {
        let observer = WindowObserver::new(callback);
        let id = observer.id;
        self.observers.push(observer);
        id
    }

    /// Removes an observer by ID.
    ///
    /// Returns true if the observer was found and removed.
    pub fn remove_observer(&mut self, id: u64) -> bool {
        if let Some(pos) = self.observers.iter().position(|o| o.id == id) {
            self.observers.remove(pos);
            true
        } else {
            false
        }
    }
}

impl WindowTree {
    /// Notifies all observers of an event with explicit flags.
    pub fn notify_with_flags(
        &mut self,
        win: WindowId,
        notify_type: NotifyWindow,
        flags: WindowNotifyFlags,
    ) {
        let observers = self.get(win).observers.clone();
        if observers.is_empty() {
            return;
        }
        let event = EventWindow { win, flags };
        for observer in observers {
            (observer.callback)(notify_type, &event, self);
        }
    }

    /// Notifies all windows in the tree that have state changes.
    ///
    /// This emits visibility/size/move change notifications after reflow.
    /// Only windows whose state differs from their old state get notified.
    pub fn notify_all(&mut self, win: WindowId) {
        let (flags, children) = {
            let win_ref = self.get(win);
            let was_visible = self.was_visible(win);
            let is_visible = self.is_visible(win);
            let flags = win_ref.compute_notify_flags_with_visibility(was_visible, is_visible);
            (flags, win_ref.children.clone())
        };

        if !flags.is_empty() {
            self.notify_with_flags(win, NotifyWindow::State, flags);
        }

        for child in children {
            self.notify_all(child);
        }

        let state = self.get(win).state;
        self.get_mut(win).old = state;
    }

    /// Updates old state to match current state for this window and all descendants.
    ///
    /// Call this after notifications to reset the tracking state.
    pub fn update_old_state(&mut self, win: WindowId) {
        {
            let win_ref = self.get_mut(win);
            win_ref.old = win_ref.state;
        }

        let children = self.get(win).children.clone();
        for child in children {
            self.update_old_state(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell as StdRefCell;

    use crate::window::WindowOrientation;
    use crate::window::WindowSize;
    use crate::window::WindowType;

    thread_local! {
        static OBSERVED: StdRefCell<Vec<(NotifyWindow, WindowNotifyFlags, WindowId)>> = const { StdRefCell::new(Vec::new()) };
        static ORDER: StdRefCell<Vec<&'static str>> = const { StdRefCell::new(Vec::new()) };
        static REMOVE_ID: StdRefCell<Option<u64>> = const { StdRefCell::new(None) };
    }

    fn record_observer(notify_type: NotifyWindow, event: &EventWindow, _tree: &mut WindowTree) {
        OBSERVED.with(|observed| {
            observed
                .borrow_mut()
                .push((notify_type, event.flags, event.win));
        });
    }

    fn observer_a(_notify_type: NotifyWindow, _event: &EventWindow, _tree: &mut WindowTree) {
        ORDER.with(|order| order.borrow_mut().push("a"));
    }

    fn observer_b(_notify_type: NotifyWindow, _event: &EventWindow, _tree: &mut WindowTree) {
        ORDER.with(|order| order.borrow_mut().push("b"));
    }

    fn observer_remove_self(
        _notify_type: NotifyWindow,
        event: &EventWindow,
        tree: &mut WindowTree,
    ) {
        let id = REMOVE_ID.with(|remove_id| remove_id.borrow_mut().take());
        if let Some(id) = id {
            tree.get_mut(event.win).remove_observer(id);
        }
    }

    fn observer_add_child(_notify_type: NotifyWindow, event: &EventWindow, tree: &mut WindowTree) {
        let child = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            1,
            1,
        );
        tree.add_child(event.win, child);
    }

    #[test]
    fn window_notify_flags() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        tree.get_mut(win).old = super::super::WindowState::new(crate::geom::Size::new(80, 24));
        tree.get_mut(win).state = super::super::WindowState::new(crate::geom::Size::new(100, 30));

        let flags = tree.get(win).compute_notify_flags();
        assert!(flags.contains(WindowNotifyFlags::TALLER));
        assert!(flags.contains(WindowNotifyFlags::WIDER));
    }

    #[test]
    fn window_notify_flags_with_ancestor_visibility() {
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
            5,
        );

        tree.add_child(root, child);
        {
            let root_mut = tree.get_mut(root);
            root_mut.state = super::super::WindowState::new(crate::geom::Size::new(80, 24));
            root_mut.old = super::super::WindowState::new(crate::geom::Size::new(80, 24));
        }
        {
            let child_mut = tree.get_mut(child);
            child_mut.state = super::super::WindowState::new(crate::geom::Size::new(10, 5));
            child_mut.old = super::super::WindowState::new(crate::geom::Size::new(10, 5));
            child_mut.state.visible = false;
            child_mut.state.rect.size.rows = 6;
            child_mut.state.rect.origin.col = 1;
        }

        tree.get_mut(child).add_observer(record_observer);
        OBSERVED.with(|observed| observed.borrow_mut().clear());

        tree.notify_all(root);

        OBSERVED.with(|observed| {
            let observed = observed.borrow();
            assert_eq!(observed.len(), 1);
            let (notify_type, flags, _addr) = observed[0];
            assert_eq!(notify_type, NotifyWindow::State);
            assert!(flags.contains(WindowNotifyFlags::HIDDEN));
            assert!(flags.contains(WindowNotifyFlags::TALLER));
            assert!(flags.contains(WindowNotifyFlags::MOVED));
        });
    }

    #[test]
    fn add_observer_returns_unique_ids() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let id1 = tree.get_mut(win).add_observer(record_observer);
        let id2 = tree.get_mut(win).add_observer(record_observer);
        assert_ne!(id1, id2);
    }

    #[test]
    fn remove_observer_nonexistent_returns_false() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        assert!(!tree.get_mut(win).remove_observer(9999));
    }

    #[test]
    fn notify_with_flags_empty_observers() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        tree.notify_with_flags(win, NotifyWindow::State, WindowNotifyFlags::empty());
    }

    #[test]
    fn update_old_state_propagates() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let child = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            5,
            5,
        );
        tree.add_child(root, child);

        tree.get_mut(root).state.rect.size.rows = 12;
        tree.get_mut(child).state.rect.size.cols = 6;

        tree.update_old_state(root);

        let root_borrowed = tree.get(root);
        let child_borrowed = tree.get(child);
        assert_eq!(
            root_borrowed.old.rect.size.rows,
            root_borrowed.state.rect.size.rows
        );
        assert_eq!(
            child_borrowed.old.rect.size.cols,
            child_borrowed.state.rect.size.cols
        );
    }

    #[test]
    fn notify_with_flags_observer_order() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        tree.get_mut(win).add_observer(observer_a);
        tree.get_mut(win).add_observer(observer_b);
        ORDER.with(|order| order.borrow_mut().clear());
        tree.notify_with_flags(win, NotifyWindow::State, WindowNotifyFlags::empty());

        ORDER.with(|order| {
            let order = order.borrow();
            assert_eq!(&order[..], ["a", "b"]);
        });
    }

    #[test]
    fn observer_removal_during_notify() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let id = tree.get_mut(win).add_observer(observer_remove_self);
        REMOVE_ID.with(|remove_id| *remove_id.borrow_mut() = Some(id));
        tree.get_mut(win).add_observer(observer_b);

        ORDER.with(|order| order.borrow_mut().clear());
        tree.notify_with_flags(win, NotifyWindow::State, WindowNotifyFlags::empty());

        ORDER.with(|order| order.borrow_mut().clear());
        tree.notify_with_flags(win, NotifyWindow::State, WindowNotifyFlags::empty());
        ORDER.with(|order| {
            let order = order.borrow();
            assert_eq!(&order[..], ["b"]);
        });
    }

    #[test]
    fn observer_modifies_window_tree() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        tree.get_mut(win).add_observer(observer_add_child);
        let initial = tree.get(win).children.len();
        tree.notify_with_flags(win, NotifyWindow::State, WindowNotifyFlags::empty());
        assert_eq!(tree.get(win).children.len(), initial + 1);
    }

    #[test]
    fn notify_all_with_no_observers() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        tree.get_mut(win).state.rect.size.rows = 12;
        tree.notify_all(win);
        assert_eq!(tree.get(win).old.rect.size.rows, 12);
    }
}
