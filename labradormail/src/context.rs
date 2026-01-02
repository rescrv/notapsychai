//! GUI context holding all runtime state.
//!
//! This module eliminates global/static state by collecting all mutable
//! runtime state into a single `GuiContext` struct that gets passed through
//! the call stack.

use std::cell::RefCell;
use std::rc::Rc;
use std::rc::Weak;

use crossterm::style::Attribute;

use crate::mutt_curses::AttrColor;
use crate::mutt_curses::ColorId;
use crate::window::CursorState;
use crate::window::MuttWindow;

/// Storage for the color palette.
#[derive(Debug)]
struct SimpleColorStore {
    colors: Vec<AttrColor>,
}

impl Default for SimpleColorStore {
    fn default() -> Self {
        let mut colors = vec![AttrColor::unset(); ColorId::Max as usize];
        let mut set = |id: ColorId, attrs: &[Attribute]| {
            colors[id as usize] = AttrColor::new(None, None, attrs);
        };

        set(ColorId::Bold, &[Attribute::Bold]);
        set(ColorId::Indicator, &[Attribute::Reverse]);
        set(ColorId::Italic, &[Attribute::Italic]);
        set(ColorId::Markers, &[Attribute::Reverse]);
        set(ColorId::Search, &[Attribute::Reverse]);
        set(ColorId::Status, &[Attribute::Reverse]);
        set(ColorId::StripeEven, &[Attribute::Bold]);
        set(ColorId::Underline, &[Attribute::Underlined]);

        Self { colors }
    }
}

impl SimpleColorStore {
    fn get(&self, cid: ColorId) -> AttrColor {
        if cid == ColorId::None || cid == ColorId::Max {
            return AttrColor::unset();
        }

        self.colors
            .get(cid as usize)
            .cloned()
            .unwrap_or_else(AttrColor::unset)
    }

    fn set(&mut self, cid: ColorId, color: AttrColor) {
        if cid == ColorId::None || cid == ColorId::Max {
            return;
        }
        if let Some(entry) = self.colors.get_mut(cid as usize) {
            *entry = color;
        }
    }
}

/// GUI context holding all runtime state.
///
/// This struct replaces all global/static mutable state in the GUI crate.
/// It should be created once at application startup and passed through
/// the call stack to functions that need access to GUI state.
pub struct GuiContext {
    /// Color palette storage.
    colors: SimpleColorStore,
    /// Saved cursor state.
    saved_cursor: CursorState,
    /// Current terminal color.
    current_color: AttrColor,
    /// Weak reference to the root window.
    root_window: Option<Weak<RefCell<MuttWindow>>>,
}

impl Default for GuiContext {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiContext {
    /// Creates a new GUI context with default state.
    pub fn new() -> Self {
        Self {
            colors: SimpleColorStore::default(),
            saved_cursor: CursorState::Visible,
            current_color: AttrColor::unset(),
            root_window: None,
        }
    }

    /// Gets a color from the palette.
    pub fn simple_color_get(&self, cid: ColorId) -> AttrColor {
        self.colors.get(cid)
    }

    /// Checks if a color is set in the palette.
    pub fn simple_color_is_set(&self, cid: ColorId) -> bool {
        self.simple_color_get(cid).is_set
    }

    /// Sets a color in the palette.
    pub fn simple_color_set(&mut self, cid: ColorId, color: AttrColor) -> AttrColor {
        self.colors.set(cid, color.clone());
        color
    }

    /// Resets a color to its default value.
    pub fn simple_color_reset(&mut self, cid: ColorId) {
        let defaults = SimpleColorStore::default();
        self.colors.set(cid, defaults.get(cid));
    }

    /// Resets all colors to their default values.
    pub fn simple_color_reset_all(&mut self) {
        self.colors = SimpleColorStore::default();
    }

    /// Gets the current terminal color.
    pub fn current_color(&self) -> &AttrColor {
        &self.current_color
    }

    /// Sets the current terminal color.
    pub fn set_current_color(&mut self, color: AttrColor) {
        self.current_color = color;
    }

    /// Gets the saved cursor state.
    pub fn saved_cursor(&self) -> CursorState {
        self.saved_cursor
    }

    /// Sets the saved cursor state.
    pub fn set_saved_cursor(&mut self, state: CursorState) {
        self.saved_cursor = state;
    }

    /// Registers the root window.
    pub fn register_root_window(&mut self, root: &Rc<RefCell<MuttWindow>>) {
        self.root_window = Some(Rc::downgrade(root));
    }

    /// Gets a reference to the root window if it exists.
    pub fn root_window(&self) -> Option<Rc<RefCell<MuttWindow>>> {
        self.root_window.as_ref().and_then(|weak| weak.upgrade())
    }

    /// Clears the root window reference.
    pub fn clear_root_window(&mut self) {
        self.root_window = None;
    }

    /// Returns a merged color: overlay merged over normal.
    pub fn merge_with_normal(&self, overlay: &AttrColor) -> AttrColor {
        let normal = self.simple_color_get(ColorId::Normal);
        AttrColor::merged_over(&normal, overlay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::style::Attribute;
    use crossterm::style::Color;

    #[test]
    fn context_default_colors() {
        let ctx = GuiContext::new();
        let status = ctx.simple_color_get(ColorId::Status);
        assert!(status.is_set);
        assert!(status.attrs.contains(&Attribute::Reverse));
    }

    #[test]
    fn context_set_and_get_color() {
        let mut ctx = GuiContext::new();
        let custom = AttrColor::new(Some(Color::Red), Some(Color::Blue), &[Attribute::Bold]);
        ctx.simple_color_set(ColorId::Message, custom.clone());
        assert_eq!(ctx.simple_color_get(ColorId::Message), custom);
    }

    #[test]
    fn context_reset_color() {
        let mut ctx = GuiContext::new();
        let custom = AttrColor::new(Some(Color::Red), None, &[]);
        ctx.simple_color_set(ColorId::Status, custom);
        ctx.simple_color_reset(ColorId::Status);

        let expected = AttrColor::new(None, None, &[Attribute::Reverse]);
        assert_eq!(ctx.simple_color_get(ColorId::Status), expected);
    }

    #[test]
    fn context_cursor_state() {
        let mut ctx = GuiContext::new();
        assert_eq!(ctx.saved_cursor(), CursorState::Visible);

        ctx.set_saved_cursor(CursorState::Invisible);
        assert_eq!(ctx.saved_cursor(), CursorState::Invisible);
    }

    #[test]
    fn context_current_color() {
        let mut ctx = GuiContext::new();
        let color = AttrColor::new(Some(Color::Green), None, &[]);
        ctx.set_current_color(color.clone());
        assert_eq!(ctx.current_color(), &color);
    }

    #[test]
    fn simple_color_get_none_and_max_unset() {
        let ctx = GuiContext::new();
        assert!(!ctx.simple_color_get(ColorId::None).is_set);
        assert!(!ctx.simple_color_get(ColorId::Max).is_set);
    }

    #[test]
    fn simple_color_set_ignores_none_and_max() {
        let mut ctx = GuiContext::new();
        let color = AttrColor::new(Some(Color::Blue), None, &[]);
        ctx.simple_color_set(ColorId::None, color.clone());
        ctx.simple_color_set(ColorId::Max, color);
        assert!(!ctx.simple_color_get(ColorId::None).is_set);
        assert!(!ctx.simple_color_get(ColorId::Max).is_set);
    }

    #[test]
    fn simple_color_reset_all_restores_defaults() {
        let mut ctx = GuiContext::new();
        let custom = AttrColor::new(Some(Color::Red), None, &[]);
        ctx.simple_color_set(ColorId::Status, custom);
        ctx.simple_color_reset_all();
        let status = ctx.simple_color_get(ColorId::Status);
        assert!(status.is_set);
        assert!(status.attrs.contains(&Attribute::Reverse));
    }

    #[test]
    fn root_window_registration_lifecycle() {
        let mut ctx = GuiContext::new();
        let root = MuttWindow::new(
            crate::window::WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        ctx.register_root_window(&root);
        assert!(ctx.root_window().is_some());
        drop(root);
        assert!(ctx.root_window().is_none());
        ctx.clear_root_window();
        assert!(ctx.root_window().is_none());
    }

    #[test]
    fn merge_with_normal_uses_unset_normal() {
        let mut ctx = GuiContext::new();
        let overlay = AttrColor::new(Some(Color::Yellow), None, &[Attribute::Bold]);
        ctx.simple_color_set(ColorId::Normal, AttrColor::unset());
        let merged = ctx.merge_with_normal(&overlay);
        assert_eq!(merged, overlay);
    }

    #[test]
    fn merge_with_normal_merges_attrs_and_colors() {
        let mut ctx = GuiContext::new();
        let normal = AttrColor::new(Some(Color::Blue), None, &[Attribute::Bold]);
        ctx.simple_color_set(ColorId::Normal, normal.clone());
        let overlay = AttrColor::new(None, Some(Color::Red), &[Attribute::Italic]);
        let merged = ctx.merge_with_normal(&overlay);
        assert_eq!(merged.fg, normal.fg);
        assert_eq!(merged.bg, Color::Red);
        assert!(merged.attrs.contains(&Attribute::Bold));
        assert!(merged.attrs.contains(&Attribute::Italic));
    }
}
