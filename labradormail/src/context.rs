//! GUI context holding all runtime state.
//!
//! This module eliminates global/static state by collecting all mutable
//! runtime state into a single `GuiContext` struct that gets passed through
//! the call stack.

use std::io::Result;
use std::io::Write;

use crossterm::style::Attribute;
use crossterm::style::Color;
use crossterm::style::ResetColor;
use crossterm::style::SetAttribute;
use crossterm::style::SetBackgroundColor;
use crossterm::style::SetForegroundColor;
use crossterm::ExecutableCommand;

use crate::window::CursorState;
use crate::window::WindowId;

/// Color IDs that map to palette entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ColorId {
    /// No color.
    None = 0,
    /// MIME attachments text (entire line).
    Attachment,
    /// MIME attachment text (takes a pattern).
    AttachHeaders,
    /// Pager body highlight (takes a pattern).
    Body,
    /// Bold text.
    Bold,
    /// Header labels, e.g. From:
    ComposeHeader,
    /// Compose security both.
    ComposeSecurityBoth,
    /// Compose security encrypt.
    ComposeSecurityEncrypt,
    /// Compose security none.
    ComposeSecurityNone,
    /// Compose security sign.
    ComposeSecuritySign,
    /// Error message.
    Error,
    /// Header default color.
    HdrDefault,
    /// Header patterns.
    Header,
    /// Selected item in list.
    Indicator,
    /// Italic text.
    Italic,
    /// Pager markers.
    Markers,
    /// Informational message.
    Message,
    /// Plain text.
    Normal,
    /// Options in prompt.
    Options,
    /// Progress bar.
    Progress,
    /// Question/user input.
    Prompt,
    /// Quoted text level 0.
    Quoted0,
    /// Quoted text level 1.
    Quoted1,
    /// Quoted text level 2.
    Quoted2,
    /// Quoted text level 3.
    Quoted3,
    /// Quoted text level 4.
    Quoted4,
    /// Quoted text level 5.
    Quoted5,
    /// Quoted text level 6.
    Quoted6,
    /// Quoted text level 7.
    Quoted7,
    /// Quoted text level 8.
    Quoted8,
    /// Quoted text level 9.
    Quoted9,
    /// Search matches.
    Search,
    /// Sidebar background.
    SidebarBackground,
    /// Sidebar divider.
    SidebarDivider,
    /// Sidebar flagged.
    SidebarFlagged,
    /// Sidebar highlight.
    SidebarHighlight,
    /// Sidebar indicator.
    SidebarIndicator,
    /// Sidebar new.
    SidebarNew,
    /// Sidebar ordinary.
    SidebarOrdinary,
    /// Sidebar spool file.
    SidebarSpoolFile,
    /// Sidebar unread.
    SidebarUnread,
    /// Signature lines.
    Signature,
    /// Status bar.
    Status,
    /// Help stripes even.
    StripeEven,
    /// Help stripes odd.
    StripeOdd,
    /// Pager tildes.
    Tilde,
    /// Index tree glyphs.
    Tree,
    /// Underlined text.
    Underline,
    /// Warning messages.
    Warning,
    /// Index default (pattern).
    Index,
    /// Index author.
    IndexAuthor,
    /// Index collapsed.
    IndexCollapsed,
    /// Index date.
    IndexDate,
    /// Index flags.
    IndexFlags,
    /// Index label.
    IndexLabel,
    /// Index number.
    IndexNumber,
    /// Index size.
    IndexSize,
    /// Index subject.
    IndexSubject,
    /// Index tag.
    IndexTag,
    /// Index tags.
    IndexTags,
    /// Sentinel.
    Max,
}

/// Color + attribute bundle used by curses-like callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrColor {
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Foreground explicitly set.
    pub fg_set: bool,
    /// Background explicitly set.
    pub bg_set: bool,
    /// Attributes to apply.
    pub attrs: Vec<Attribute>,
    /// Whether this color entry is configured.
    pub is_set: bool,
}

impl AttrColor {
    /// Creates an unset color entry.
    pub fn unset() -> Self {
        Self {
            fg: Color::Reset,
            bg: Color::Reset,
            fg_set: false,
            bg_set: false,
            attrs: Vec::new(),
            is_set: false,
        }
    }

    /// Creates a color entry with optional foreground/background and attributes.
    pub fn new(fg: Option<Color>, bg: Option<Color>, attrs: &[Attribute]) -> Self {
        let fg_set = fg.is_some();
        let bg_set = bg.is_some();
        let is_set = fg_set || bg_set || !attrs.is_empty();
        Self {
            fg: fg.unwrap_or(Color::Reset),
            bg: bg.unwrap_or(Color::Reset),
            fg_set,
            bg_set,
            attrs: attrs.to_vec(),
            is_set,
        }
    }

    /// Applies this color and attributes to the terminal.
    pub fn apply(&self, out: &mut dyn Write) -> Result<()> {
        out.execute(SetAttribute(Attribute::Reset))?;
        out.execute(ResetColor)?;
        if self.fg_set {
            out.execute(SetForegroundColor(self.fg))?;
        }
        if self.bg_set {
            out.execute(SetBackgroundColor(self.bg))?;
        }
        for attr in &self.attrs {
            out.execute(SetAttribute(*attr))?;
        }
        Ok(())
    }

    /// Merges an overlay color over a base color.
    pub fn merged_over(base: &AttrColor, overlay: &AttrColor) -> AttrColor {
        if !overlay.is_set {
            return base.clone();
        }
        if !base.is_set {
            return overlay.clone();
        }

        let mut attrs = Vec::new();
        for attr in base.attrs.iter().chain(overlay.attrs.iter()) {
            if !attrs.contains(attr) {
                attrs.push(*attr);
            }
        }

        AttrColor {
            fg: if overlay.fg_set { overlay.fg } else { base.fg },
            bg: if overlay.bg_set { overlay.bg } else { base.bg },
            fg_set: overlay.fg_set || base.fg_set,
            bg_set: overlay.bg_set || base.bg_set,
            attrs,
            is_set: overlay.is_set || base.is_set,
        }
    }
}

impl Default for AttrColor {
    fn default() -> Self {
        Self::unset()
    }
}

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
    /// Root window ID.
    root_window: Option<WindowId>,
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
    pub fn register_root_window(&mut self, root: WindowId) {
        self.root_window = Some(root);
    }

    /// Gets the root window ID if it exists.
    pub fn root_window(&self) -> Option<WindowId> {
        self.root_window
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

    /// Sets the color/attribute state for subsequent output.
    pub fn set_color(&mut self, out: &mut dyn Write, ac: &AttrColor) -> Result<()> {
        ac.apply(out)?;
        self.set_current_color(ac.clone());
        Ok(())
    }

    /// Sets the color by palette ID (fallbacks to normal if unset).
    pub fn set_color_by_id(&mut self, out: &mut dyn Write, cid: ColorId) -> Result<AttrColor> {
        let mut ac = self.simple_color_get(cid);
        if !ac.is_set {
            ac = self.simple_color_get(ColorId::Normal);
        }
        self.set_color(out, &ac)?;
        Ok(ac)
    }

    /// Sets color by palette ID, merged over normal.
    pub fn set_normal_backed_color_by_id(
        &mut self,
        out: &mut dyn Write,
        cid: ColorId,
    ) -> Result<AttrColor> {
        let normal = self.simple_color_get(ColorId::Normal);
        let overlay = self.simple_color_get(cid);
        let merged = AttrColor::merged_over(&normal, &overlay);
        self.set_color(out, &merged)?;
        Ok(merged)
    }

    /// Sets the cursor state and returns the previous one.
    pub fn set_cursor(&mut self, out: &mut dyn Write, state: CursorState) -> Result<CursorState> {
        let old = self.saved_cursor();
        self.set_saved_cursor(state);

        if let Err(err) = state.set(out) {
            if state == CursorState::Visible {
                let _ = CursorState::VeryVisible.set(out);
            } else {
                return Err(err);
            }
        }

        Ok(old)
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
        let mut tree = crate::window::WindowTree::new();
        let root = tree.add_window(
            crate::window::WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        ctx.register_root_window(root);
        assert!(ctx.root_window().is_some());
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
