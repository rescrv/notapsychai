//! Window observer and notification support.

use std::cell::RefCell;
use std::rc::Rc;
use std::rc::Weak;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use bitflags::bitflags;

use super::MuttWindow;

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
#[derive(Debug)]
pub struct EventWindow {
    /// Weak reference to the window.
    pub win: Weak<RefCell<MuttWindow>>,
    /// Notification flags.
    pub flags: WindowNotifyFlags,
}

/// Global counter for generating unique observer IDs.
static NEXT_OBSERVER_ID: AtomicU64 = AtomicU64::new(1);

/// Type alias for window observer callbacks.
pub type WindowObserverFn = fn(NotifyWindow, &EventWindow);

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
/// Extracts the observer ID from typed window data and removes the observer.
/// Returns true if an observer was found and removed.
pub fn handle_observer_delete<T: HasObserverId + 'static>(win: &Rc<RefCell<MuttWindow>>) -> bool {
    let observer_id = {
        let mut borrowed = win.borrow_mut();
        borrowed
            .wdata_mut::<T>()
            .and_then(|data| data.take_observer_id())
    };
    if let Some(id) = observer_id {
        win.borrow_mut().remove_observer(id);
        return true;
    }
    false
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

impl MuttWindow {
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

        if self.state.rows > self.old.rows {
            flags |= WindowNotifyFlags::TALLER;
        } else if self.state.rows < self.old.rows {
            flags |= WindowNotifyFlags::SHORTER;
        }

        if self.state.cols > self.old.cols {
            flags |= WindowNotifyFlags::WIDER;
        } else if self.state.cols < self.old.cols {
            flags |= WindowNotifyFlags::NARROWER;
        }

        if self.state.row_offset != self.old.row_offset
            || self.state.col_offset != self.old.col_offset
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

    /// Notifies all observers of an event.
    pub fn notify(&self, notify_type: NotifyWindow, win_weak: Weak<RefCell<MuttWindow>>) {
        let flags = self.compute_notify_flags();
        self.notify_with_flags(notify_type, win_weak, flags);
    }

    /// Notifies all observers of an event with explicit flags.
    pub fn notify_with_flags(
        &self,
        notify_type: NotifyWindow,
        win_weak: Weak<RefCell<MuttWindow>>,
        flags: WindowNotifyFlags,
    ) {
        let event = EventWindow {
            win: win_weak,
            flags,
        };
        for observer in &self.observers {
            (observer.callback)(notify_type, &event);
        }
    }

    /// Notifies all observers without holding a RefCell borrow during callbacks.
    pub fn notify_with_flags_rc(
        win: &Rc<RefCell<MuttWindow>>,
        notify_type: NotifyWindow,
        flags: WindowNotifyFlags,
    ) {
        let (observers, win_weak) = {
            let borrowed = win.borrow();
            (borrowed.observers.clone(), Rc::downgrade(win))
        };
        if observers.is_empty() {
            return;
        }
        let event = EventWindow {
            win: win_weak,
            flags,
        };
        for observer in observers {
            (observer.callback)(notify_type, &event);
        }
    }

    /// Notifies all windows in the tree that have state changes.
    ///
    /// This emits visibility/size/move change notifications after reflow.
    /// Only windows whose state differs from their old state get notified.
    pub fn notify_all(win: &Rc<RefCell<Self>>) {
        let (flags, children) = {
            let borrowed = win.borrow();
            let was_visible = borrowed.was_visible();
            let is_visible = borrowed.is_visible();
            let flags = borrowed.compute_notify_flags_with_visibility(was_visible, is_visible);
            (flags, borrowed.children.to_vec())
        };

        if !flags.is_empty() {
            Self::notify_with_flags_rc(win, NotifyWindow::State, flags);
        }

        for child in children {
            Self::notify_all(&child);
        }

        // Update old state after notifications (matching NeoMutt's visibility rules)
        let state = win.borrow().state;
        win.borrow_mut().old = state;
    }

    /// Updates old state to match current state for this window and all descendants.
    ///
    /// Call this after notifications to reset the tracking state.
    pub fn update_old_state(win: &Rc<RefCell<Self>>) {
        {
            let mut borrowed = win.borrow_mut();
            borrowed.old = borrowed.state;
        }

        let borrowed = win.borrow();
        for child in &borrowed.children {
            Self::update_old_state(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell as StdRefCell;

    use crate::window::MuttWindow;
    use crate::window::WindowOrientation;
    use crate::window::WindowSize;
    use crate::window::WindowType;

    thread_local! {
        static OBSERVED: StdRefCell<Vec<(NotifyWindow, WindowNotifyFlags, usize)>> = const { StdRefCell::new(Vec::new()) };
        static ORDER: StdRefCell<Vec<&'static str>> = const { StdRefCell::new(Vec::new()) };
        static REMOVE_ID: StdRefCell<Option<u64>> = const { StdRefCell::new(None) };
    }

    fn record_observer(notify_type: NotifyWindow, event: &EventWindow) {
        let addr = event
            .win
            .upgrade()
            .map(|w| Rc::as_ptr(&w) as usize)
            .unwrap_or_default();
        OBSERVED.with(|observed| {
            observed.borrow_mut().push((notify_type, event.flags, addr));
        });
    }

    fn observer_a(_notify_type: NotifyWindow, _event: &EventWindow) {
        ORDER.with(|order| order.borrow_mut().push("a"));
    }

    fn observer_b(_notify_type: NotifyWindow, _event: &EventWindow) {
        ORDER.with(|order| order.borrow_mut().push("b"));
    }

    fn observer_remove_self(_notify_type: NotifyWindow, event: &EventWindow) {
        if let Some(win) = event.win.upgrade() {
            let id = REMOVE_ID.with(|remove_id| remove_id.borrow_mut().take());
            if let Some(id) = id {
                win.borrow_mut().remove_observer(id);
            }
        }
    }

    fn observer_add_child(_notify_type: NotifyWindow, event: &EventWindow) {
        if let Some(win) = event.win.upgrade() {
            let child = MuttWindow::new(
                WindowType::Container,
                WindowOrientation::Vertical,
                WindowSize::Fixed,
                1,
                1,
            );
            MuttWindow::add_child(&win, child);
        }
    }

    #[test]
    fn window_notify_flags() {
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        win.borrow_mut().old = super::super::WindowState::new(80, 24);
        win.borrow_mut().state = super::super::WindowState::new(100, 30);

        let flags = win.borrow().compute_notify_flags();
        assert!(flags.contains(WindowNotifyFlags::TALLER));
        assert!(flags.contains(WindowNotifyFlags::WIDER));
    }

    #[test]
    fn window_notify_flags_with_ancestor_visibility() {
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
            5,
        );

        MuttWindow::add_child(&root, Rc::clone(&child));
        {
            let mut root_mut = root.borrow_mut();
            root_mut.state = super::super::WindowState::new(80, 24);
            root_mut.old = super::super::WindowState::new(80, 24);
        }
        {
            let mut child_mut = child.borrow_mut();
            child_mut.state = super::super::WindowState::new(10, 5);
            child_mut.old = super::super::WindowState::new(10, 5);
            child_mut.state.visible = false;
            child_mut.state.rows = 6;
            child_mut.state.col_offset = 1;
        }

        child.borrow_mut().add_observer(record_observer);
        OBSERVED.with(|observed| observed.borrow_mut().clear());

        MuttWindow::notify_all(&root);

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
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let id1 = win.borrow_mut().add_observer(record_observer);
        let id2 = win.borrow_mut().add_observer(record_observer);
        assert_ne!(id1, id2);
    }

    #[test]
    fn remove_observer_nonexistent_returns_false() {
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        assert!(!win.borrow_mut().remove_observer(9999));
    }

    #[test]
    fn notify_with_flags_rc_empty_observers() {
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        MuttWindow::notify_with_flags_rc(&win, NotifyWindow::State, WindowNotifyFlags::empty());
    }

    #[test]
    fn update_old_state_propagates() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let child = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            5,
            5,
        );
        MuttWindow::add_child(&root, Rc::clone(&child));

        root.borrow_mut().state.rows = 12;
        child.borrow_mut().state.cols = 6;

        MuttWindow::update_old_state(&root);

        let root_borrowed = root.borrow();
        let child_borrowed = child.borrow();
        assert_eq!(root_borrowed.old.rows, root_borrowed.state.rows);
        assert_eq!(child_borrowed.old.cols, child_borrowed.state.cols);
    }

    #[test]
    fn notify_with_flags_rc_observer_order() {
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        win.borrow_mut().add_observer(observer_a);
        win.borrow_mut().add_observer(observer_b);
        ORDER.with(|order| order.borrow_mut().clear());
        MuttWindow::notify_with_flags_rc(&win, NotifyWindow::State, WindowNotifyFlags::empty());

        ORDER.with(|order| {
            let order = order.borrow();
            assert_eq!(&order[..], ["a", "b"]);
        });
    }

    #[test]
    fn observer_removal_during_notify() {
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        let id = win.borrow_mut().add_observer(observer_remove_self);
        REMOVE_ID.with(|remove_id| *remove_id.borrow_mut() = Some(id));
        win.borrow_mut().add_observer(observer_b);

        ORDER.with(|order| order.borrow_mut().clear());
        MuttWindow::notify_with_flags_rc(&win, NotifyWindow::State, WindowNotifyFlags::empty());

        ORDER.with(|order| order.borrow_mut().clear());
        MuttWindow::notify_with_flags_rc(&win, NotifyWindow::State, WindowNotifyFlags::empty());
        ORDER.with(|order| {
            let order = order.borrow();
            assert_eq!(&order[..], ["b"]);
        });
    }

    #[test]
    fn observer_modifies_window_tree() {
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        win.borrow_mut().add_observer(observer_add_child);
        let initial = win.borrow().children.len();
        MuttWindow::notify_with_flags_rc(&win, NotifyWindow::State, WindowNotifyFlags::empty());
        assert_eq!(win.borrow().children.len(), initial + 1);
    }

    #[test]
    fn notify_all_with_no_observers() {
        let win = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );
        win.borrow_mut().state.rows = 12;
        MuttWindow::notify_all(&win);
        assert_eq!(win.borrow().old.rows, 12);
    }
}
