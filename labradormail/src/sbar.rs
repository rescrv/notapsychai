//! Simple status bar implementation.
//!
//! The status bar displays a title at the bottom of dialogs.

use std::cell::RefCell;
use std::io::Result;
use std::io::Write;
use std::rc::Rc;

use crate::context::GuiContext;
use crate::curs_lib::mutt_paddstr_string;
use crate::mutt_curses::mutt_curses_merge_with_normal;
use crate::mutt_curses::mutt_curses_set_color;
use crate::mutt_curses::mutt_curses_set_color_by_id;
use crate::mutt_curses::mutt_curses_set_normal_backed_color_by_id;
use crate::mutt_curses::AttrColor;
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

/// Simple status bar private data.
#[derive(Debug, Clone, Default)]
pub struct SBarPrivateData {
    /// Cached display string (owned).
    pub display: String,
    /// Truncated/padded string ready for display.
    pub formatted: String,
    /// Foreground color for display.
    pub color: AttrColor,
    /// Observer ID for window events.
    pub window_observer_id: Option<u64>,
}

impl SBarPrivateData {
    /// Creates new status bar data.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the display text.
    pub fn set_title(&mut self, title: &str) {
        self.display = title.to_string();
    }

    /// Gets the display text.
    pub fn title(&self) -> &str {
        &self.display
    }

    /// Formats the title to fit within the given width.
    ///
    /// Truncates if too long, pads with spaces if too short.
    /// This matches NeoMutt's mutt_paddstr behavior.
    pub fn format_to_width(&mut self, width: usize) {
        self.formatted = mutt_paddstr_string(&self.display, width);
    }

    /// Gets the formatted display string.
    pub fn formatted(&self) -> &str {
        &self.formatted
    }

    /// Sets the status bar color.
    pub fn set_color(&mut self, color: AttrColor) {
        self.color = color;
    }
}

impl HasObserverId for SBarPrivateData {
    fn take_observer_id(&mut self) -> Option<u64> {
        self.window_observer_id.take()
    }
}

/// Status bar window wrapper.
pub struct StatusBar {
    window: Rc<RefCell<MuttWindow>>,
}

impl StatusBar {
    /// Creates a new status bar.
    pub fn new() -> Self {
        let window = MuttWindow::new(
            WindowType::StatusBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            1,
        );

        {
            let mut borrowed = window.borrow_mut();
            borrowed.wdata = Some(Box::new(SBarPrivateData::new()));
            borrowed.recalc = Some(sbar_recalc);
            borrowed.repaint = Some(sbar_repaint);
            borrowed.draw = Some(sbar_draw);
        }
        let observer_id = window.borrow_mut().add_observer(sbar_window_observer);
        {
            let mut borrowed = window.borrow_mut();
            if let Some(data) = borrowed.wdata_mut::<SBarPrivateData>() {
                data.window_observer_id = Some(observer_id);
            }
        }

        Self { window }
    }

    /// Returns a reference to the underlying window.
    pub fn window(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.window
    }

    /// Sets the status bar title.
    pub fn set_title(&self, title: &str) {
        let mut borrowed = self.window.borrow_mut();
        if let Some(data) = borrowed.wdata_mut::<SBarPrivateData>() {
            data.set_title(title);
        }
        borrowed.actions |= WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
    }

    /// Sets status bar color and triggers repaint.
    pub fn set_color(&self, color: AttrColor) {
        let mut borrowed = self.window.borrow_mut();
        if let Some(data) = borrowed.wdata_mut::<SBarPrivateData>() {
            data.set_color(color);
        }
        borrowed.actions |= WindowActionFlags::REPAINT;
    }

    /// Marks the status bar for repaint after a color configuration change.
    pub fn notify_color_change(&self) {
        self.window.borrow_mut().actions |= WindowActionFlags::REPAINT;
    }

    /// Gets the status bar title.
    pub fn title(&self) -> Option<String> {
        let borrowed = self.window.borrow();
        borrowed
            .wdata_ref::<SBarPrivateData>()
            .map(|data| data.title().to_string())
    }

    /// Gets the formatted display string.
    pub fn formatted(&self) -> Option<String> {
        let borrowed = self.window.borrow();
        borrowed
            .wdata_ref::<SBarPrivateData>()
            .map(|data| data.formatted().to_string())
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Recalculate callback for status bar.
///
/// Following NeoMutt's pattern: recalc requests repaint.
fn sbar_recalc(win: &mut MuttWindow) {
    win.actions |= WindowActionFlags::REPAINT;
}

/// Repaint callback for status bar.
///
/// Formats the title to fit the window width (truncate/pad).
/// In NeoMutt this draws directly using curses. In our implementation,
/// we prepare the formatted string for the draw phase.
fn sbar_repaint(win: &mut MuttWindow) {
    let width = win.state.cols as usize;

    if let Some(data) = win.wdata_mut::<SBarPrivateData>() {
        data.format_to_width(width);
    }
}

fn sbar_draw(win: &mut MuttWindow, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
    let Some(data) = win.wdata_ref::<SBarPrivateData>() else {
        return Ok(());
    };
    let color = data.color.clone();
    let formatted = data.formatted.clone();

    win.move_cursor(out, 0, 0)?;
    if color.is_set {
        let merged = mutt_curses_merge_with_normal(ctx, &color);
        mutt_curses_set_color(ctx, out, &merged)?;
    } else {
        mutt_curses_set_normal_backed_color_by_id(ctx, out, ColorId::Status)?;
    }
    win.addstr(out, &formatted)?;
    mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
    win.clrtoeol(ctx, out)?;
    Ok(())
}

fn sbar_window_observer(notify_type: NotifyWindow, event: &EventWindow) {
    let win = match event.win.upgrade() {
        Some(win) => win,
        None => return,
    };

    match notify_type {
        NotifyWindow::State => {
            win.borrow_mut().actions |= WindowActionFlags::REPAINT;
        }
        NotifyWindow::Delete => {
            handle_observer_delete::<SBarPrivateData>(&win);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GuiContext;

    #[test]
    fn sbar_data_set_title() {
        let mut data = SBarPrivateData::new();
        data.set_title("Test Title");
        assert_eq!(data.title(), "Test Title");
    }

    #[test]
    fn status_bar_create() {
        let sbar = StatusBar::new();
        assert_eq!(sbar.window().borrow().req_rows, 1);
        assert_eq!(sbar.window().borrow().window_type, WindowType::StatusBar);
    }

    #[test]
    fn status_bar_set_title() {
        let sbar = StatusBar::new();
        sbar.set_title("My Status");

        let title = sbar.title();
        assert!(title.is_some());
        assert_eq!(title.unwrap(), "My Status");
    }

    #[test]
    fn sbar_format_padding() {
        let mut data = SBarPrivateData::new();
        data.set_title("Hello");
        data.format_to_width(10);

        assert_eq!(data.formatted(), "Hello     ");
        assert_eq!(data.formatted().len(), 10);
    }

    #[test]
    fn sbar_format_truncation() {
        let mut data = SBarPrivateData::new();
        data.set_title("Hello World");
        data.format_to_width(5);

        assert_eq!(data.formatted(), "Hello");
        assert_eq!(data.formatted().len(), 5);
    }

    #[test]
    fn sbar_format_exact_fit() {
        let mut data = SBarPrivateData::new();
        data.set_title("Hello");
        data.format_to_width(5);

        assert_eq!(data.formatted(), "Hello");
    }

    #[test]
    fn sbar_format_empty() {
        let mut data = SBarPrivateData::new();
        data.set_title("");
        data.format_to_width(5);

        assert_eq!(data.formatted(), "     ");
    }

    #[test]
    fn sbar_format_zero_width() {
        let mut data = SBarPrivateData::new();
        data.set_title("Hello");
        data.format_to_width(0);

        assert_eq!(data.formatted(), "");
    }

    #[test]
    fn sbar_format_wide_chars() {
        let mut data = SBarPrivateData::new();
        // Chinese characters are typically width 2
        data.set_title("\u{4E2D}\u{6587}"); // "中文"
        data.format_to_width(6);

        // "中文" is 4 cells wide, so should be padded with 2 spaces
        assert_eq!(data.formatted(), "\u{4E2D}\u{6587}  ");
    }

    #[test]
    fn sbar_format_wide_chars_truncation() {
        let mut data = SBarPrivateData::new();
        // Chinese characters are typically width 2
        data.set_title("\u{4E2D}\u{6587}\u{6D4B}\u{8BD5}"); // "中文测试"
        data.format_to_width(5);

        // Only "中文" (4 cells) fits, plus 1 space padding
        assert_eq!(data.formatted(), "\u{4E2D}\u{6587} ");
    }

    #[test]
    fn sbar_repaint_formats() {
        let sbar = StatusBar::new();
        sbar.set_title("Test");

        // Simulate window with known width
        {
            let mut win = sbar.window().borrow_mut();
            win.state.cols = 10;
        }

        // Call repaint
        {
            let mut win = sbar.window().borrow_mut();
            sbar_repaint(&mut win);
        }

        let formatted = sbar.formatted();
        assert!(formatted.is_some());
        assert_eq!(formatted.unwrap(), "Test      ");
    }

    #[test]
    fn sbar_draw_outputs_title() {
        let sbar = StatusBar::new();
        sbar.set_title("Status");
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        {
            let mut win = sbar.window().borrow_mut();
            win.state.cols = 10;
            win.state.rows = 1;
            sbar_repaint(&mut win);
            sbar_draw(&mut win, &mut ctx, &mut out).unwrap();
        }
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Status"));
    }
}
