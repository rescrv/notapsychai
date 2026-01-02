//! Message container stack helpers.
//!
//! This mirrors NeoMutt's msgcont module for managing stacked message windows.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use crate::context::GuiContext;
use crate::message_window::MessageWindow;
use crate::window::MuttWindow;
use crate::window::WindowActionFlags;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowType;
use crate::window_reflow;

/// Message container stack.
pub struct MessageContainer {
    container: Rc<RefCell<MuttWindow>>,
    base_msgwin: Rc<RefCell<MuttWindow>>,
}

impl MessageContainer {
    /// Creates a new message container with a base message window.
    pub fn new(cols: i16) -> Self {
        let container = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Minimise,
            cols,
            1,
        );

        let msgwin = MessageWindow::new(false);
        let base_msgwin = Rc::clone(msgwin.window());
        MuttWindow::add_child(&container, Rc::clone(&base_msgwin));

        Self {
            container,
            base_msgwin,
        }
    }

    /// Returns the container window.
    pub fn window(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.container
    }

    /// Returns the base message window.
    pub fn base_window(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.base_msgwin
    }

    /// Pushes a message window onto the stack.
    pub fn push(
        &self,
        win: Rc<RefCell<MuttWindow>>,
        ctx: Option<&mut GuiContext>,
        out: Option<&mut dyn Write>,
    ) {
        if let Some(top) = self.top() {
            top.borrow_mut().set_visible(false);
        }

        MuttWindow::add_child(&self.container, win);
        window_reflow(&self.container);
        let root = MuttWindow::get_root(&self.container);
        MuttWindow::invalidate_all(&root);
        if let (Some(ctx), Some(out)) = (ctx, out) {
            let _ = MuttWindow::redraw(&root, ctx, out);
        }
    }

    /// Pops the top message window from the stack.
    pub fn pop(
        &self,
        ctx: Option<&mut GuiContext>,
        out: Option<&mut dyn Write>,
    ) -> Option<Rc<RefCell<MuttWindow>>> {
        let mut container = self.container.borrow_mut();

        if container.children.len() <= 1 {
            return None;
        }

        let popped = container.children.pop();
        if let Some(ref win) = popped {
            win.borrow_mut().set_visible(false);
        }

        if let Some(new_top) = container.children.last() {
            new_top.borrow_mut().set_visible(true);
            new_top.borrow_mut().actions |= WindowActionFlags::RECALC;
        }

        drop(container);
        window_reflow(&self.container);
        let root = MuttWindow::get_root(&self.container);
        MuttWindow::invalidate_all(&root);
        if let (Some(ctx), Some(out)) = (ctx, out) {
            let _ = MuttWindow::redraw(&root, ctx, out);
        }
        popped
    }

    /// Returns the top window.
    pub fn top(&self) -> Option<Rc<RefCell<MuttWindow>>> {
        self.container.borrow().children.last().cloned()
    }

    /// Returns the base message window (msgcont_get_msgwin).
    pub fn get_msgwin(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.base_msgwin
    }

    /// Hides the top message window.
    pub fn hide_top(&self) {
        if let Some(top) = self.top() {
            MuttWindow::set_visible_with_parent(&top, false);
        }
        window_reflow(&self.container);
    }

    /// Shows the top message window.
    pub fn show_top(&self) {
        if let Some(top) = self.top() {
            MuttWindow::set_visible_with_parent(&top, true);
        }
        window_reflow(&self.container);
    }

    /// Returns the stack length.
    pub fn len(&self) -> usize {
        self.container.borrow().children.len()
    }

    /// Returns true if the container is empty.
    pub fn is_empty(&self) -> bool {
        self.container.borrow().children.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msgcont_push_pop() {
        let msgcont = MessageContainer::new(80);
        assert_eq!(msgcont.len(), 1);

        let new_msgwin = MessageWindow::new(true);
        msgcont.push(Rc::clone(new_msgwin.window()), None, None);
        assert_eq!(msgcont.len(), 2);

        assert!(!msgcont.base_window().borrow().state.visible);
        assert!(new_msgwin.window().borrow().state.visible);

        let popped = msgcont.pop(None, None);
        assert!(popped.is_some());
        assert_eq!(msgcont.len(), 1);
        assert!(msgcont.base_window().borrow().state.visible);
    }

    #[test]
    fn msgcont_push_requests_redraw() {
        let msgcont = MessageContainer::new(80);
        {
            let mut container = msgcont.window().borrow_mut();
            container.actions = WindowActionFlags::empty();
        }

        let new_msgwin = MessageWindow::new(true);
        msgcont.push(Rc::clone(new_msgwin.window()), None, None);

        let container = msgcont.window().borrow();
        assert!(container.actions.contains(WindowActionFlags::RECALC));
        assert!(container.actions.contains(WindowActionFlags::REPAINT));
    }
}
