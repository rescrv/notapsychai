//! Help dialog implementation.
//!
//! Displays a simple list of key bindings in a dialog window.

use std::io::Result;
use std::io::Write;

use crate::action::Action;
use crate::context::ColorId;
use crate::context::GuiContext;
use crate::curs_lib::pad_string;
use crate::curs_lib::ScrollState;
use crate::help_data::HelpData;
use crate::help_data::HelpItem;
use crate::render::Renderer;
use crate::sbar::StatusBar;
use crate::window::CursorBehavior;
use crate::window::RenderMode;
use crate::window::WindowId;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowTree;
use crate::window::WindowType;
use crate::window::WindowWidget;

#[derive(Debug, Clone)]
pub struct HelpDialogContentData {
    items: Vec<HelpItem>,
    formatted: Vec<String>,
    last_width: i16,
    scroll: ScrollState,
}

impl HelpDialogContentData {
    fn new(items: Vec<HelpItem>) -> Self {
        Self {
            items,
            formatted: Vec::new(),
            last_width: -1,
            scroll: ScrollState::new(),
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
            self.formatted.push(pad_string(&line, width as usize));
        }
    }
}

impl HelpDialogContentData {
    pub fn update(&mut self, tree: &mut WindowTree, win: WindowId) {
        let width = tree.get(win).state.rect.size.cols;
        self.ensure_formatted(width);
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
        let width = tree.get(win).state.rect.size.cols;
        let rows = tree.get(win).state.rect.size.rows;
        self.ensure_formatted(width);
        let lines = self.formatted.clone();
        let scroll_offset = self.scroll.offset();
        let width = width.max(0) as usize;
        let mut renderer = Renderer::new(ctx, out);
        for row in 0..rows {
            renderer.move_cursor(tree.get(win), row, 0)?;
            let line_idx = scroll_offset + row as usize;
            let color = if line_idx.is_multiple_of(2) {
                ColorId::StripeEven
            } else {
                ColorId::StripeOdd
            };
            renderer.set_color_by_id(color)?;

            let line = lines
                .get(line_idx)
                .cloned()
                .unwrap_or_else(|| " ".repeat(width));
            renderer.write_str(&line)?;
        }
        Ok(CursorBehavior::Hidden)
    }
}

/// Help dialog window wrapper.
pub struct HelpDialog {
    window: WindowId,
}

impl HelpDialog {
    /// Creates a new help dialog for the given help data.
    pub fn new(tree: &mut WindowTree, help_data: &HelpData) -> Self {
        let dialog = tree.add_dialog(WindowType::DlgHelp);
        {
            let borrowed = tree.get_mut(dialog);
            borrowed.help_data = Some(HelpData::from_items(vec![HelpItem::new("q", "Close help")]));
        }
        let content = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        {
            let borrowed = tree.get_mut(content);
            borrowed.set_widget(WindowWidget::HelpDialog(HelpDialogContentData::new(
                help_data.items.clone(),
            )));
        }

        let sbar = StatusBar::new(tree);
        sbar.set_title(tree, "Help - press q to close");

        tree.add_child(dialog, content);
        tree.add_child(dialog, sbar.window_id());

        Self { window: dialog }
    }

    /// Returns the underlying dialog window ID.
    pub fn window_id(&self) -> WindowId {
        self.window
    }
}

/// Scrolls the help dialog content if the window is a help dialog.
///
/// Returns true if the scroll was handled.
pub fn help_dialog_scroll(tree: &mut WindowTree, win: WindowId, action: Action) -> bool {
    if tree.get(win).window_type != WindowType::DlgHelp {
        return false;
    }

    let content = tree
        .get(win)
        .children
        .iter()
        .copied()
        .find(|child| tree.get(*child).window_type == WindowType::Custom);

    let Some(content) = content else {
        return false;
    };

    let visible_rows = tree.get(content).state.rect.size.rows.max(0) as usize;
    let Some(WindowWidget::HelpDialog(data)) = tree.get_mut(content).widget_mut() else {
        return false;
    };

    let max_offset = ScrollState::max_offset(data.formatted.len(), visible_rows);
    match action {
        Action::NextEntry | Action::NextLine => {
            data.scroll.scroll_down(1, max_offset);
        }
        Action::PrevEntry | Action::PrevLine => {
            data.scroll.scroll_up(1);
        }
        Action::NextPage | Action::HalfDown => {
            let amount = visible_rows.max(1) / 2;
            data.scroll.scroll_down(amount, max_offset);
        }
        Action::PrevPage | Action::HalfUp => {
            let amount = visible_rows.max(1) / 2;
            data.scroll.scroll_up(amount);
        }
        Action::FirstEntry | Action::TopPage => {
            data.scroll.scroll_to_top();
        }
        Action::LastEntry | Action::BottomPage => {
            data.scroll
                .scroll_to_bottom(visible_rows, data.formatted.len());
        }
        _ => return false,
    }

    tree.get_mut(content).mark_repaint();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::WindowActionFlags;

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
        let mut tree = WindowTree::new();
        let dialog = HelpDialog::new(&mut tree, &data);
        let win = dialog.window_id();
        assert_eq!(tree.get(win).window_type, WindowType::DlgHelp);
        assert_eq!(tree.get(win).children.len(), 2);
        assert!(tree
            .get(win)
            .help_data
            .as_ref()
            .map(|help| !help.items.is_empty())
            .unwrap_or(false));
        assert!(tree
            .get(win)
            .children
            .iter()
            .any(|child| tree.get(*child).window_type == WindowType::Custom));
        assert!(tree
            .get(win)
            .children
            .iter()
            .any(|child| tree.get(*child).window_type == WindowType::StatusBar));
    }

    #[test]
    fn help_dialog_recalc_requests_repaint() {
        let items = vec![HelpItem::new("q", "Quit")];
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            1,
        );
        {
            let borrowed = tree.get_mut(win);
            borrowed.set_widget(WindowWidget::HelpDialog(HelpDialogContentData::new(items)));
            borrowed.actions = WindowActionFlags::RECALC;
            borrowed.state.visible = true;
        }
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        // Use redraw to trigger recalc through the widget path.
        tree.redraw(win, &mut ctx, &mut out).unwrap();
        // After recalc, REPAINT should have been added but then consumed by the draw
        // We verify the widget was formatted instead
        let data = if let Some(WindowWidget::HelpDialog(data)) = tree.get(win).widget_ref() {
            data
        } else {
            panic!("expected help dialog widget data")
        };
        assert_eq!(data.last_width, 10);
    }

    #[test]
    fn help_dialog_draw_formats_lines() {
        let items = vec![HelpItem::new("q", "Quit"), HelpItem::new("x", "Exit")];
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            12,
            2,
        );
        {
            let borrowed = tree.get_mut(win);
            borrowed.set_widget(WindowWidget::HelpDialog(HelpDialogContentData::new(items)));
            borrowed.state.rect.size.cols = 12;
            borrowed.state.rect.size.rows = 2;
            borrowed.state.visible = true;
            borrowed.mark_recalc_repaint();
        }

        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        // Use redraw to trigger recalc and draw through the widget path.
        tree.redraw(win, &mut ctx, &mut out).unwrap();

        let data = if let Some(WindowWidget::HelpDialog(data)) = tree.get(win).widget_ref() {
            data
        } else {
            panic!("expected help dialog widget data")
        };
        assert_eq!(data.formatted.len(), 2);
        assert_eq!(data.formatted[0].len(), 12);
        assert_eq!(data.formatted[1].len(), 12);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Quit"));
    }

    #[test]
    fn scroll_down_and_up() {
        let mut data = HelpDialogContentData::new(vec![
            HelpItem::new("a", "A"),
            HelpItem::new("b", "B"),
            HelpItem::new("c", "C"),
            HelpItem::new("d", "D"),
            HelpItem::new("e", "E"),
        ]);
        data.ensure_formatted(20);

        let visible_rows = 3usize;
        let max_offset = ScrollState::max_offset(data.formatted.len(), visible_rows);

        assert_eq!(data.scroll.offset(), 0);

        // Scroll down by 1 with 3 visible rows
        data.scroll.scroll_down(1, max_offset);
        assert_eq!(data.scroll.offset(), 1);

        // Scroll down more
        data.scroll.scroll_down(1, max_offset);
        assert_eq!(data.scroll.offset(), 2);

        // Can't scroll past max (5 items - 3 visible = max offset 2)
        data.scroll.scroll_down(10, max_offset);
        assert_eq!(data.scroll.offset(), 2);

        // Scroll up
        data.scroll.scroll_up(1);
        assert_eq!(data.scroll.offset(), 1);

        // Scroll up past 0
        data.scroll.scroll_up(10);
        assert_eq!(data.scroll.offset(), 0);
    }

    #[test]
    fn help_dialog_scroll_handles_actions() {
        let items: Vec<HelpItem> = (0..20)
            .map(|i| HelpItem::new(format!("{}", i), format!("Item {}", i)))
            .collect();
        let data = HelpData::from_items(items);
        let mut tree = WindowTree::new();
        let dialog = HelpDialog::new(&mut tree, &data);

        // Set content window size
        {
            let content = tree
                .get(dialog.window_id())
                .children
                .iter()
                .copied()
                .find(|c| tree.get(*c).window_type == WindowType::Custom)
                .unwrap();
            let content_borrowed = tree.get_mut(content);
            content_borrowed.state.rect.size.rows = 5;
            content_borrowed.state.rect.size.cols = 20;
            if let Some(WindowWidget::HelpDialog(d)) = content_borrowed.widget_mut() {
                d.ensure_formatted(20);
            }
        }

        // Scroll down
        assert!(help_dialog_scroll(
            &mut tree,
            dialog.window_id(),
            Action::NextEntry
        ));

        // Verify scroll happened
        {
            let content = tree
                .get(dialog.window_id())
                .children
                .iter()
                .copied()
                .find(|c| tree.get(*c).window_type == WindowType::Custom)
                .unwrap();
            let content_borrowed = tree.get(content);
            let d = if let Some(WindowWidget::HelpDialog(d)) = content_borrowed.widget_ref() {
                d
            } else {
                panic!("expected help dialog widget data")
            };
            assert_eq!(d.scroll.offset(), 1);
        }

        // Non-help dialog returns false
        let non_help = tree.add_dialog(WindowType::DlgIndex);
        assert!(!help_dialog_scroll(&mut tree, non_help, Action::NextEntry));
    }
}
