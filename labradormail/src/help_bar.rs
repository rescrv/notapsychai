//! Help bar window implementation.
//!
//! The help bar displays key bindings for the focused window.

use std::cell::RefCell;
use std::io::Result;
use std::io::Write;
use std::rc::Rc;

use crate::context::GuiContext;
use crate::curs_lib::mutt_paddstr;
use crate::help_data::HelpData;
use crate::mutt_curses::mutt_curses_set_color_by_id;
use crate::mutt_curses::mutt_curses_set_normal_backed_color_by_id;
use crate::mutt_curses::ColorId;
use crate::window::handle_observer_delete;
use crate::window::EventWindow;
use crate::window::HasObserverId;
use crate::window::MuttWindow;
use crate::window::NotifyWindow;
use crate::window::WindowActionFlags;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowType;

/// Help bar private data.
#[derive(Debug, Clone, Default)]
pub struct HelpBarWindowData {
    /// Cached compiled help string.
    pub help_str: String,
    /// Last help data used to compute the string.
    pub help_data: Option<Rc<HelpData>>,
    /// Observer ID for window events.
    pub window_observer_id: Option<u64>,
}

impl HasObserverId for HelpBarWindowData {
    fn take_observer_id(&mut self) -> Option<u64> {
        self.window_observer_id.take()
    }
}

/// Help bar window wrapper.
pub struct HelpBar {
    window: Rc<RefCell<MuttWindow>>,
}

impl HelpBar {
    /// Creates a new help bar window.
    pub fn new() -> Self {
        let window = MuttWindow::new(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            1,
        );

        {
            let mut borrowed = window.borrow_mut();
            borrowed.wdata = Some(Box::new(HelpBarWindowData::default()));
            borrowed.recalc = Some(helpbar_recalc);
            borrowed.repaint = Some(helpbar_repaint);
            borrowed.draw = Some(helpbar_draw);
        }
        let observer_id = window.borrow_mut().add_observer(helpbar_window_observer);
        {
            let mut borrowed = window.borrow_mut();
            if let Some(data) = borrowed.wdata_mut::<HelpBarWindowData>() {
                data.window_observer_id = Some(observer_id);
            }
        }

        Self { window }
    }

    /// Returns a reference to the underlying window.
    pub fn window(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.window
    }
}

impl Default for HelpBar {
    fn default() -> Self {
        Self::new()
    }
}

fn helpbar_recalc(win: &mut MuttWindow) {
    let help_info = {
        let mut root = match win.parent.as_ref().and_then(|weak| weak.upgrade()) {
            Some(parent) => parent,
            None => return,
        };
        loop {
            let parent = root
                .borrow()
                .parent
                .as_ref()
                .and_then(|weak| weak.upgrade());
            if let Some(parent) = parent {
                root = parent;
            } else {
                break;
            }
        }

        let focus = MuttWindow::get_focus(&root);
        find_help_data(&focus)
    };

    if let Some(data) = win.wdata_mut::<HelpBarWindowData>() {
        if let Some(help_data) = help_info {
            data.help_data = Some(Rc::clone(&help_data));
            data.help_str = help_data.compile_line();
        } else {
            data.help_data = None;
            data.help_str.clear();
        }
    }

    win.actions |= WindowActionFlags::REPAINT;
}

fn helpbar_repaint(_win: &mut MuttWindow) {}

fn helpbar_draw(win: &mut MuttWindow, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
    let Some(data) = win.wdata_ref::<HelpBarWindowData>() else {
        return Ok(());
    };
    let help_str = data.help_str.clone();

    let width = win.state.cols.max(0) as usize;
    mutt_curses_set_normal_backed_color_by_id(ctx, out, ColorId::Status)?;
    win.move_cursor(out, 0, 0)?;
    mutt_paddstr(win, out, width, &help_str)?;
    mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
    Ok(())
}

fn helpbar_window_observer(notify_type: NotifyWindow, event: &EventWindow) {
    let win = match event.win.upgrade() {
        Some(win) => win,
        None => return,
    };

    match notify_type {
        NotifyWindow::State => {
            win.borrow_mut().actions |= WindowActionFlags::RECALC;
        }
        NotifyWindow::Delete => {
            handle_observer_delete::<HelpBarWindowData>(&win);
        }
        _ => {}
    }
}

fn find_help_data(focus: &Rc<RefCell<MuttWindow>>) -> Option<Rc<HelpData>> {
    let mut current = Rc::clone(focus);
    loop {
        let borrowed = current.borrow();
        if let Some(ref help_data) = borrowed.help_data {
            return Some(Rc::clone(help_data));
        }
        let parent = borrowed.parent.as_ref().and_then(|weak| weak.upgrade());
        drop(borrowed);
        if let Some(parent) = parent {
            current = parent;
        } else {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GuiContext;
    use crate::help_data::HelpItem;
    use crate::window::WindowType;

    #[test]
    fn helpbar_recalc_uses_focus_help_data() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let helpbar = HelpBar::new();
        let dialog = MuttWindow::new(
            WindowType::DlgIndex,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        let help_data = Rc::new(HelpData::from_items(vec![HelpItem::new("q", "Quit")]));
        dialog.borrow_mut().help_data = Some(Rc::clone(&help_data));

        MuttWindow::add_child(&root, Rc::clone(helpbar.window()));
        MuttWindow::add_child(&root, Rc::clone(&dialog));
        MuttWindow::set_focus(&dialog);

        let mut win = helpbar.window().borrow_mut();
        helpbar_recalc(&mut win);

        let data = win.wdata_ref::<HelpBarWindowData>().unwrap();
        assert_eq!(data.help_str, "q: Quit");
    }

    #[test]
    fn helpbar_draw_outputs_help_text() {
        let root = MuttWindow::new(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            40,
            3,
        );
        let helpbar = HelpBar::new();
        let dialog = MuttWindow::new(
            WindowType::DlgIndex,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let help_data = Rc::new(HelpData::from_items(vec![HelpItem::new("q", "Quit")]));
        dialog.borrow_mut().help_data = Some(Rc::clone(&help_data));

        MuttWindow::add_child(&root, Rc::clone(helpbar.window()));
        MuttWindow::add_child(&root, Rc::clone(&dialog));
        MuttWindow::set_focus(&dialog);

        {
            let mut win = helpbar.window().borrow_mut();
            win.state.cols = 40;
            win.state.rows = 1;
            helpbar_recalc(&mut win);
        }

        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        let mut win = helpbar.window().borrow_mut();
        helpbar_draw(&mut win, &mut ctx, &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("q: Quit"));
    }
}
