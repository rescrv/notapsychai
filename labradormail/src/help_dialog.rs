//! Help dialog implementation.
//!
//! Displays a simple list of key bindings in a dialog window.

use std::cell::RefCell;
use std::io::Result;
use std::io::Write;
use std::rc::Rc;

use crossterm::style::Print;
use crossterm::ExecutableCommand;

use crate::context::GuiContext;
use crate::curs_lib::mutt_paddstr_string;
use crate::dialog::Dialog;
use crate::help_data::HelpData;
use crate::help_data::HelpItem;
use crate::mutt_curses::mutt_curses_set_color_by_id;
use crate::mutt_curses::ColorId;
use crate::sbar::StatusBar;
use crate::window::MuttWindow;
use crate::window::WindowActionFlags;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowType;

#[derive(Debug, Clone)]
struct HelpDialogContentData {
    items: Vec<HelpItem>,
    formatted: Vec<String>,
    last_width: i16,
}

impl HelpDialogContentData {
    fn new(items: Vec<HelpItem>) -> Self {
        Self {
            items,
            formatted: Vec::new(),
            last_width: -1,
        }
    }

    fn ensure_formatted(&mut self, width: i16) {
        if width == self.last_width {
            return;
        }
        self.last_width = width;
        self.formatted.clear();

        if width <= 0 {
            return;
        }

        let key_width = self
            .items
            .iter()
            .map(|item| item.key.len())
            .max()
            .unwrap_or(0);

        for item in &self.items {
            let line = format!(
                "{:width$}  {}",
                item.key,
                item.description,
                width = key_width
            );
            self.formatted
                .push(mutt_paddstr_string(&line, width as usize));
        }
    }
}

/// Help dialog window wrapper.
pub struct HelpDialog {
    window: Rc<RefCell<MuttWindow>>,
}

impl HelpDialog {
    /// Creates a new help dialog for the given help data.
    pub fn new(help_data: &HelpData) -> Self {
        let dialog = Dialog::new(WindowType::DlgHelp);
        {
            let mut borrowed = dialog.window().borrow_mut();
            borrowed.help_data = Some(Rc::new(HelpData::from_items(vec![HelpItem::new(
                "q",
                "Close help",
            )])));
        }
        let content = MuttWindow::new(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        {
            let mut borrowed = content.borrow_mut();
            borrowed.wdata = Some(Box::new(HelpDialogContentData::new(
                help_data.items.clone(),
            )));
            borrowed.recalc = Some(help_dialog_recalc);
            borrowed.draw = Some(help_dialog_draw);
        }

        let sbar = StatusBar::new();
        sbar.set_title("Help - press q to close");

        Dialog::add_child(&dialog, Rc::clone(&content));
        Dialog::add_child(&dialog, Rc::clone(sbar.window()));

        Self {
            window: Rc::clone(dialog.window()),
        }
    }

    /// Returns a reference to the underlying dialog window.
    pub fn window(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.window
    }
}

fn help_dialog_recalc(win: &mut MuttWindow) {
    let width = win.state.cols;
    if let Some(data) = win.wdata_mut::<HelpDialogContentData>() {
        data.ensure_formatted(width);
    }
    win.actions |= WindowActionFlags::REPAINT;
}

fn help_dialog_draw(win: &mut MuttWindow, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
    let width = win.state.cols;
    let Some(data) = win.wdata_mut::<HelpDialogContentData>() else {
        return Ok(());
    };
    data.ensure_formatted(width);
    let lines = data.formatted.clone();
    let width = width.max(0) as usize;

    for row in 0..win.state.rows {
        win.move_cursor(out, row, 0)?;
        let color = if row % 2 == 0 {
            ColorId::StripeEven
        } else {
            ColorId::StripeOdd
        };
        mutt_curses_set_color_by_id(ctx, out, color)?;

        let line = lines
            .get(row as usize)
            .cloned()
            .unwrap_or_else(|| " ".repeat(width));
        out.execute(Print(line))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_dialog_content_new() {
        let items = vec![HelpItem::new("q", "Quit")];
        let data = HelpDialogContentData::new(items.clone());
        assert_eq!(data.items, items);
        assert!(data.formatted.is_empty());
        assert_eq!(data.last_width, -1);
    }

    #[test]
    fn ensure_formatted_basic() {
        let items = vec![HelpItem::new("q", "Quit"), HelpItem::new("help", "Help")];
        let mut data = HelpDialogContentData::new(items);
        data.ensure_formatted(20);
        assert_eq!(data.formatted.len(), 2);
        for line in &data.formatted {
            assert_eq!(line.len(), 20);
        }
        assert!(data.formatted[0].contains("q"));
        assert!(data.formatted[1].contains("help"));
    }

    #[test]
    fn ensure_formatted_width_change_reformats() {
        let items = vec![HelpItem::new("q", "Quit")];
        let mut data = HelpDialogContentData::new(items);
        data.ensure_formatted(12);
        let first = data.formatted.clone();
        data.ensure_formatted(8);
        let second = data.formatted.clone();
        assert_ne!(first, second);
        assert_eq!(second[0].len(), 8);
    }

    #[test]
    fn ensure_formatted_empty_items() {
        let mut data = HelpDialogContentData::new(Vec::new());
        data.ensure_formatted(10);
        assert!(data.formatted.is_empty());
    }

    #[test]
    fn ensure_formatted_narrow_width() {
        let items = vec![HelpItem::new("verylongkey", "Desc")];
        let mut data = HelpDialogContentData::new(items);
        data.ensure_formatted(5);
        assert_eq!(data.formatted.len(), 1);
        assert_eq!(data.formatted[0].len(), 5);
    }

    #[test]
    fn help_dialog_new_builds_tree() {
        let data = HelpData::from_items(vec![HelpItem::new("q", "Quit")]);
        let dialog = HelpDialog::new(&data);
        let win = dialog.window();
        let borrowed = win.borrow();
        assert_eq!(borrowed.window_type, WindowType::DlgHelp);
        assert_eq!(borrowed.children.len(), 2);
        assert!(borrowed
            .help_data
            .as_ref()
            .map(|help| !help.items.is_empty())
            .unwrap_or(false));
        assert!(borrowed
            .children
            .iter()
            .any(|child| child.borrow().window_type == WindowType::Custom));
        assert!(borrowed
            .children
            .iter()
            .any(|child| child.borrow().window_type == WindowType::StatusBar));
    }

    #[test]
    fn help_dialog_recalc_requests_repaint() {
        let items = vec![HelpItem::new("q", "Quit")];
        let win = MuttWindow::new(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            1,
        );
        {
            let mut borrowed = win.borrow_mut();
            borrowed.wdata = Some(Box::new(HelpDialogContentData::new(items)));
            borrowed.actions = WindowActionFlags::NONE;
        }
        let mut borrowed = win.borrow_mut();
        help_dialog_recalc(&mut borrowed);
        assert!(borrowed.actions.contains(WindowActionFlags::REPAINT));
    }

    #[test]
    fn help_dialog_draw_formats_lines() {
        let items = vec![HelpItem::new("q", "Quit"), HelpItem::new("x", "Exit")];
        let win = MuttWindow::new(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            12,
            2,
        );
        {
            let mut borrowed = win.borrow_mut();
            borrowed.wdata = Some(Box::new(HelpDialogContentData::new(items)));
            borrowed.state.cols = 12;
            borrowed.state.rows = 2;
        }

        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        let mut borrowed = win.borrow_mut();
        help_dialog_draw(&mut borrowed, &mut ctx, &mut out).unwrap();

        let data = borrowed.wdata_ref::<HelpDialogContentData>().unwrap();
        assert_eq!(data.formatted.len(), 2);
        assert_eq!(data.formatted[0].len(), 12);
        assert_eq!(data.formatted[1].len(), 12);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Quit"));
    }
}
