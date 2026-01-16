//! Simple status bar implementation.
//!
//! The status bar displays a title at the bottom of dialogs.

use std::io::Result;
use std::io::Write;

use crate::context::AttrColor;
use crate::context::ColorId;
use crate::context::GuiContext;
use crate::curs_lib::pad_string;
use crate::render::Renderer;
use crate::window::CursorBehavior;
use crate::window::EventWindow;
use crate::window::HasObserverId;
use crate::window::NotifyWindow;
use crate::window::RenderMode;
use crate::window::WindowId;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowTree;
use crate::window::WindowType;
use crate::window::WindowWidget;

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
    /// This matches NeoMutt's padding behavior.
    pub fn format_to_width(&mut self, width: usize) {
        self.formatted = pad_string(&self.display, width);
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

impl SBarPrivateData {
    pub fn update(&mut self, tree: &mut WindowTree, win: WindowId) {
        tree.get_mut(win).mark_repaint();
    }

    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        if matches!(mode, RenderMode::CursorOnly) {
            return Ok(CursorBehavior::Hidden);
        }
        let width = tree.get(win).state.rect.size.cols as usize;
        self.format_to_width(width);

        let color = self.color.clone();
        let formatted = self.formatted.clone();

        if color.is_set {
            let merged = ctx.merge_with_normal(&color);
            ctx.set_color(out, &merged)?;
        } else {
            ctx.set_normal_backed_color_by_id(out, ColorId::Status)?;
        }
        {
            let mut renderer = Renderer::new(ctx, out);
            renderer.draw_single_row_bar(tree.get(win), &formatted)?;
        }
        ctx.set_color_by_id(out, ColorId::Normal)?;
        Ok(CursorBehavior::Hidden)
    }
}

/// Status bar window wrapper.
pub struct StatusBar {
    window: WindowId,
}

impl StatusBar {
    /// Creates a new status bar.
    pub fn new(tree: &mut WindowTree) -> Self {
        let window = tree.add_window(
            WindowType::StatusBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            1,
        );

        {
            let borrowed = tree.get_mut(window);
            borrowed.set_widget(WindowWidget::StatusBar(SBarPrivateData::new()));
        }
        let observer_id = tree.get_mut(window).add_observer(status_bar_observer);
        {
            let borrowed = tree.get_mut(window);
            if let Some(WindowWidget::StatusBar(data)) = borrowed.widget_mut() {
                data.window_observer_id = Some(observer_id);
            }
        }

        Self { window }
    }

    /// Returns a reference to the underlying window.
    pub fn window_id(&self) -> WindowId {
        self.window
    }

    /// Sets the status bar title.
    pub fn set_title(&self, tree: &mut WindowTree, title: &str) {
        let borrowed = tree.get_mut(self.window);
        if let Some(WindowWidget::StatusBar(data)) = borrowed.widget_mut() {
            data.set_title(title);
        }
        borrowed.mark_recalc_repaint();
    }

    /// Sets status bar color and triggers repaint.
    pub fn set_color(&self, tree: &mut WindowTree, color: AttrColor) {
        let borrowed = tree.get_mut(self.window);
        if let Some(WindowWidget::StatusBar(data)) = borrowed.widget_mut() {
            data.set_color(color);
        }
        borrowed.mark_repaint();
    }

    /// Marks the status bar for repaint after a color configuration change.
    pub fn notify_color_change(&self, tree: &mut WindowTree) {
        tree.get_mut(self.window).mark_repaint();
    }

    /// Gets the status bar title.
    pub fn title(&self, tree: &WindowTree) -> Option<String> {
        if let Some(WindowWidget::StatusBar(data)) = tree.get(self.window).widget_ref() {
            Some(data.title().to_string())
        } else {
            None
        }
    }

    /// Gets the formatted display string.
    pub fn formatted(&self, tree: &WindowTree) -> Option<String> {
        if let Some(WindowWidget::StatusBar(data)) = tree.get(self.window).widget_ref() {
            Some(data.formatted().to_string())
        } else {
            None
        }
    }
}

fn status_bar_observer(notify_type: NotifyWindow, event: &EventWindow, tree: &mut WindowTree) {
    match notify_type {
        NotifyWindow::State => {
            tree.get_mut(event.win).mark_repaint();
        }
        NotifyWindow::Delete => {
            let observer_id = tree
                .get_mut(event.win)
                .widget_mut()
                .and_then(WindowWidget::take_observer_id);
            if let Some(id) = observer_id {
                tree.get_mut(event.win).remove_observer(id);
            }
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
        let mut tree = WindowTree::new();
        let sbar = StatusBar::new(&mut tree);
        assert_eq!(tree.get(sbar.window_id()).req_size.rows, 1);
        assert_eq!(
            tree.get(sbar.window_id()).window_type,
            WindowType::StatusBar
        );
    }

    #[test]
    fn status_bar_set_title() {
        let mut tree = WindowTree::new();
        let sbar = StatusBar::new(&mut tree);
        sbar.set_title(&mut tree, "My Status");

        let title = sbar.title(&tree);
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
        let mut tree = WindowTree::new();
        let sbar = StatusBar::new(&mut tree);
        sbar.set_title(&mut tree, "Test");

        // Simulate window with known width
        {
            let win = tree.get_mut(sbar.window_id());
            win.state.rect.size.cols = 10;
            win.state.visible = true;
            win.mark_repaint();
        }

        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        // Use redraw to trigger repaint through the widget path.
        tree.redraw(sbar.window_id(), &mut ctx, &mut out).unwrap();

        let formatted = sbar.formatted(&tree);
        assert!(formatted.is_some());
        assert_eq!(formatted.unwrap(), "Test      ");
    }

    #[test]
    fn sbar_draw_outputs_title() {
        let mut tree = WindowTree::new();
        let sbar = StatusBar::new(&mut tree);
        sbar.set_title(&mut tree, "Status");
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        {
            let win = tree.get_mut(sbar.window_id());
            win.state.rect.size.cols = 10;
            win.state.rect.size.rows = 1;
            win.state.visible = true;
            win.mark_repaint();
        }
        // Use redraw to trigger repaint and draw through the widget path.
        tree.redraw(sbar.window_id(), &mut ctx, &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Status"));
    }
}
