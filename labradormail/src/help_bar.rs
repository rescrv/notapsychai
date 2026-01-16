//! Help bar window implementation.
//!
//! The help bar displays key bindings for the focused window.

use crate::action::Action;
use crate::context::ColorId;
use crate::context::GuiContext;
use crate::global::help_data_from_bindings;
use crate::global::Key;
use crate::help_data::HelpData;
use crate::render::Renderer;
use crate::window::standard_observer_recalc;
use crate::window::CursorBehavior;
use crate::window::HasObserverId;
use crate::window::RenderMode;
use crate::window::WindowId;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowTree;
use crate::window::WindowType;
use crate::window::WindowWidget;
use std::io::Result;
use std::io::Write;

/// Help bar private data.
#[derive(Debug, Clone, Default)]
pub struct HelpBarWindowData {
    /// Cached compiled help string.
    pub help_str: String,
    /// Last help data used to compute the string.
    pub help_data: Option<HelpData>,
    /// Observer ID for window events.
    pub window_observer_id: Option<u64>,
}

impl HasObserverId for HelpBarWindowData {
    fn take_observer_id(&mut self) -> Option<u64> {
        self.window_observer_id.take()
    }
}

impl HelpBarWindowData {
    pub fn update(&mut self, tree: &mut WindowTree, win: WindowId) {
        let (help_info, bindings) = {
            let root = match tree.get(win).parent {
                Some(parent) => tree.get_root(parent),
                None => return,
            };

            let focus = tree.get_focus(root);
            let help_data = find_help_data(tree, focus);
            let bindings = collect_bindings_from_focus(tree, focus);
            (help_data, bindings)
        };

        // Prefer static help_data if present, otherwise generate from bindings
        if let Some(help_data) = help_info {
            self.help_str = help_data.compile_line();
            self.help_data = Some(help_data);
        } else if !bindings.is_empty() {
            let generated = help_data_from_bindings(&bindings);
            self.help_str = generated.compile_line();
            self.help_data = Some(generated);
        } else {
            self.help_data = None;
            self.help_str.clear();
        }

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
        ctx.set_normal_backed_color_by_id(out, ColorId::Status)?;
        {
            let mut renderer = Renderer::new(ctx, out);
            renderer.draw_single_row_bar(tree.get(win), &self.help_str)?;
        }
        ctx.set_color_by_id(out, ColorId::Normal)?;
        Ok(CursorBehavior::Hidden)
    }
}

/// Help bar window wrapper.
pub struct HelpBar {
    window: WindowId,
}

impl HelpBar {
    /// Creates a new help bar window.
    pub fn new(tree: &mut WindowTree) -> Self {
        let window = tree.add_window(
            WindowType::HelpBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            1,
        );

        {
            let borrowed = tree.get_mut(window);
            borrowed.set_widget(WindowWidget::HelpBar(HelpBarWindowData::default()));
        }
        let observer_id = tree.get_mut(window).add_observer(standard_observer_recalc);
        {
            let borrowed = tree.get_mut(window);
            if let Some(WindowWidget::HelpBar(data)) = borrowed.widget_mut() {
                data.window_observer_id = Some(observer_id);
            }
        }

        Self { window }
    }

    /// Returns a reference to the underlying window.
    pub fn window_id(&self) -> WindowId {
        self.window
    }
}

fn find_help_data(tree: &WindowTree, focus: WindowId) -> Option<HelpData> {
    let mut current = focus;
    loop {
        let borrowed = tree.get(current);
        if let Some(ref help_data) = borrowed.help_data {
            return Some(help_data.clone());
        }
        if let Some(parent) = borrowed.parent {
            current = parent;
        } else {
            return None;
        }
    }
}

/// Collects all key bindings from the focus chain, walking from focus up to root.
///
/// Bindings closer to the focused window take precedence (appear first).
pub fn collect_bindings_from_focus(tree: &WindowTree, focus: WindowId) -> Vec<(Key, Action)> {
    let mut all_bindings = Vec::new();
    let mut current = focus;
    loop {
        let borrowed = tree.get(current);
        all_bindings.extend(borrowed.bindings.iter().copied());
        if let Some(parent) = borrowed.parent {
            current = parent;
        } else {
            break;
        }
    }
    all_bindings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::context::GuiContext;
    use crate::global::Key;
    use crate::help_data::HelpItem;
    use crate::window::WindowTree;
    use crate::window::WindowType;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyModifiers;

    /// Helper to invoke update on the help bar widget.
    fn invoke_update(tree: &mut WindowTree, win: WindowId) {
        if let Some(mut widget) = tree.get_mut(win).widget.take() {
            widget.update(tree, win);
            tree.get_mut(win).widget = Some(widget);
        }
    }

    /// Helper to invoke render on the help bar widget.
    fn invoke_render(
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        if let Some(mut widget) = tree.get_mut(win).widget.take() {
            let result = widget.render(tree, win, ctx, out, RenderMode::Paint);
            tree.get_mut(win).widget = Some(widget);
            result.map(|_| ())
        } else {
            Ok(())
        }
    }

    #[test]
    fn helpbar_recalc_uses_focus_help_data() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let helpbar = HelpBar::new(&mut tree);
        let dialog = tree.add_dialog(WindowType::DlgIndex);

        let help_data = HelpData::from_items(vec![HelpItem::new("q", "Quit")]);
        tree.get_mut(dialog).help_data = Some(help_data);

        tree.add_child(root, helpbar.window_id());
        tree.add_child(root, dialog);
        tree.set_focus(dialog);

        invoke_update(&mut tree, helpbar.window_id());

        let data =
            if let Some(WindowWidget::HelpBar(data)) = tree.get(helpbar.window_id()).widget_ref() {
                data
            } else {
                panic!("expected help bar widget data")
            };
        assert_eq!(data.help_str, "q: Quit");
    }

    #[test]
    fn helpbar_draw_outputs_help_text() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            40,
            3,
        );
        let helpbar = HelpBar::new(&mut tree);
        let dialog = tree.add_dialog(WindowType::DlgIndex);
        let help_data = HelpData::from_items(vec![HelpItem::new("q", "Quit")]);
        tree.get_mut(dialog).help_data = Some(help_data);

        tree.add_child(root, helpbar.window_id());
        tree.add_child(root, dialog);
        tree.set_focus(dialog);

        {
            let win = tree.get_mut(helpbar.window_id());
            win.state.rect.size.cols = 40;
            win.state.rect.size.rows = 1;
        }
        invoke_update(&mut tree, helpbar.window_id());

        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        invoke_render(&mut tree, helpbar.window_id(), &mut ctx, &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("q: Quit"));
    }

    #[test]
    fn helpbar_recalc_uses_bindings_when_no_help_data() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let helpbar = HelpBar::new(&mut tree);
        let dialog = tree.add_dialog(WindowType::DlgIndex);

        // Set bindings instead of help_data
        tree.get_mut(dialog).bindings.bind(
            Key::new(KeyCode::Char('q'), KeyModifiers::NONE),
            Action::Quit,
        );

        tree.add_child(root, helpbar.window_id());
        tree.add_child(root, dialog);
        tree.set_focus(dialog);

        invoke_update(&mut tree, helpbar.window_id());

        let data =
            if let Some(WindowWidget::HelpBar(data)) = tree.get(helpbar.window_id()).widget_ref() {
                data
            } else {
                panic!("expected help bar widget data")
            };
        // Generated from bindings - should contain 'q' and 'Quit' (from Action description)
        assert!(
            data.help_str.contains('q'),
            "help_str should contain 'q': {}",
            data.help_str
        );
        assert!(
            data.help_str.to_lowercase().contains("quit"),
            "help_str should contain 'quit': {}",
            data.help_str
        );
    }

    #[test]
    fn collect_bindings_gathers_from_ancestor_chain() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let container = tree.add_window(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let leaf = tree.add_window(
            WindowType::Custom,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            10,
            10,
        );

        // Set bindings at different levels
        tree.get_mut(root).bindings.bind(
            Key::new(KeyCode::Char('r'), KeyModifiers::NONE),
            Action::Help,
        );
        tree.get_mut(container).bindings.bind(
            Key::new(KeyCode::Char('c'), KeyModifiers::NONE),
            Action::Help,
        );
        tree.get_mut(leaf).bindings.bind(
            Key::new(KeyCode::Char('l'), KeyModifiers::NONE),
            Action::Quit,
        );

        tree.add_child(root, container);
        tree.add_child(container, leaf);

        let bindings = collect_bindings_from_focus(&tree, leaf);

        // Should have 3 bindings, leaf's first, then container's, then root's
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].0.code, KeyCode::Char('l'));
        assert_eq!(bindings[1].0.code, KeyCode::Char('c'));
        assert_eq!(bindings[2].0.code, KeyCode::Char('r'));
    }

    #[test]
    fn helpbar_prefers_static_help_data_over_bindings() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            80,
            24,
        );
        let helpbar = HelpBar::new(&mut tree);
        let dialog = tree.add_dialog(WindowType::DlgIndex);

        // Set both bindings and help_data
        tree.get_mut(dialog).bindings.bind(
            Key::new(KeyCode::Char('x'), KeyModifiers::NONE),
            Action::Help,
        );
        let help_data = HelpData::from_items(vec![HelpItem::new("z", "Custom")]);
        tree.get_mut(dialog).help_data = Some(help_data);

        tree.add_child(root, helpbar.window_id());
        tree.add_child(root, dialog);
        tree.set_focus(dialog);

        invoke_update(&mut tree, helpbar.window_id());

        let data =
            if let Some(WindowWidget::HelpBar(data)) = tree.get(helpbar.window_id()).widget_ref() {
                data
            } else {
                panic!("expected help bar widget data")
            };
        // Static help_data should take precedence
        assert_eq!(data.help_str, "z: Custom");
    }
}
