//! Curses wrapper utilities for color/attribute handling and resize glue.

use std::io::Result;
use std::io::Write;

use crossterm::style::Attribute;
use crossterm::style::Color;
use crossterm::style::ResetColor;
use crossterm::style::SetAttribute;
use crossterm::style::SetBackgroundColor;
use crossterm::style::SetForegroundColor;
use crossterm::ExecutableCommand;

use crate::context::GuiContext;
use crate::root_window::RootWindow;
use crate::terminal::Terminal;
use crate::window::CursorState;
use crate::window::MuttWindow;

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

/// Configuration entry for setting a single color ID.
#[derive(Debug, Clone)]
pub struct ColorConfigEntry {
    /// The color ID to configure.
    pub cid: ColorId,
    /// The foreground color, if any.
    pub fg: Option<Color>,
    /// The background color, if any.
    pub bg: Option<Color>,
    /// The text attributes to apply.
    pub attrs: Vec<Attribute>,
}

impl ColorConfigEntry {
    /// Converts the config entry into an AttrColor.
    pub fn to_attr_color(&self) -> AttrColor {
        AttrColor::new(self.fg, self.bg, &self.attrs)
    }
}

/// Applies color configuration entries to the simple color store.
///
/// This is a placeholder hook for future config parsing integration.
pub fn simple_color_apply_config(ctx: &mut GuiContext, entries: &[ColorConfigEntry]) {
    for entry in entries {
        ctx.simple_color_set(entry.cid, entry.to_attr_color());
    }
}

/// Sets the color/attribute state for subsequent output.
pub fn mutt_curses_set_color(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    ac: &AttrColor,
) -> Result<()> {
    ac.apply(out)?;
    ctx.set_current_color(ac.clone());
    Ok(())
}

/// Gets the current terminal color.
pub fn mutt_curses_current_color(ctx: &GuiContext) -> AttrColor {
    ctx.current_color().clone()
}

/// Sets the color by palette ID (fallbacks to normal if unset).
pub fn mutt_curses_set_color_by_id(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    cid: ColorId,
) -> Result<AttrColor> {
    let mut ac = ctx.simple_color_get(cid);
    if !ac.is_set {
        ac = ctx.simple_color_get(ColorId::Normal);
    }
    mutt_curses_set_color(ctx, out, &ac)?;
    Ok(ac)
}

/// Sets color by palette ID, merged over normal.
pub fn mutt_curses_set_normal_backed_color_by_id(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    cid: ColorId,
) -> Result<AttrColor> {
    let normal = ctx.simple_color_get(ColorId::Normal);
    let overlay = ctx.simple_color_get(cid);
    let merged = AttrColor::merged_over(&normal, &overlay);
    mutt_curses_set_color(ctx, out, &merged)?;
    Ok(merged)
}

/// Returns an overlay color merged over the normal color.
pub fn mutt_curses_merge_with_normal(ctx: &GuiContext, overlay: &AttrColor) -> AttrColor {
    ctx.merge_with_normal(overlay)
}

/// Sets the cursor state and returns the previous one.
pub fn mutt_curses_set_cursor(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    state: CursorState,
) -> Result<CursorState> {
    let old = ctx.saved_cursor();
    ctx.set_saved_cursor(state);

    if let Err(err) = Terminal::set_cursor(out, state) {
        if state == CursorState::Visible {
            let _ = Terminal::set_cursor(out, CursorState::VeryVisible);
        } else {
            return Err(err);
        }
    }

    Ok(old)
}

/// Refreshes screen size using terminal dimensions with LINES/COLUMNS fallback.
pub fn mutt_resize_screen(ctx: &mut GuiContext, root: &mut RootWindow) -> Result<()> {
    let (cols, rows) = Terminal::size().unwrap_or((0, 0));
    apply_resize(ctx, root, cols, rows);
    Ok(())
}

fn apply_resize(ctx: &mut GuiContext, root: &mut RootWindow, cols: u16, rows: u16) {
    let (cols, rows) = resolve_screen_size(cols, rows);
    root.set_size(cols, rows);
    ctx.register_root_window(root.root());
    MuttWindow::notify_all(root.root());
    MuttWindow::invalidate_all(root.root());
}

fn resolve_screen_size(cols: u16, rows: u16) -> (u16, u16) {
    let mut cols = cols;
    let mut rows = rows;

    if rows == 0 {
        rows = std::env::var("LINES")
            .ok()
            .and_then(|val| val.parse::<u16>().ok())
            .unwrap_or(24);
    }

    if cols == 0 {
        cols = std::env::var("COLUMNS")
            .ok()
            .and_then(|val| val.parse::<u16>().ok())
            .unwrap_or(80);
    }

    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::style::Attribute;
    use crossterm::style::Color;

    #[test]
    fn resolve_screen_size_prefers_terminal() {
        let size = resolve_screen_size(120, 40);
        assert_eq!(size, (120, 40));
    }

    #[test]
    fn resolve_screen_size_uses_env_fallbacks() {
        let old_lines = std::env::var("LINES").ok();
        let old_columns = std::env::var("COLUMNS").ok();
        std::env::set_var("LINES", "41");
        std::env::set_var("COLUMNS", "121");
        let size = resolve_screen_size(0, 0);
        assert_eq!(size, (121, 41));
        std::env::set_var("LINES", "41");
        std::env::remove_var("COLUMNS");
        let size = resolve_screen_size(0, 0);
        assert_eq!(size, (80, 41));
        std::env::remove_var("LINES");
        std::env::set_var("COLUMNS", "121");
        let size = resolve_screen_size(0, 0);
        assert_eq!(size, (121, 24));
        if let Some(value) = old_lines {
            std::env::set_var("LINES", value);
        } else {
            std::env::remove_var("LINES");
        }
        if let Some(value) = old_columns {
            std::env::set_var("COLUMNS", value);
        } else {
            std::env::remove_var("COLUMNS");
        }
    }

    #[test]
    fn apply_resize_reflows_and_updates_old_state() {
        let mut ctx = GuiContext::new();
        let mut root = RootWindow::new_with_size((80, 24)).unwrap();
        MuttWindow::update_old_state(root.root());

        apply_resize(&mut ctx, &mut root, 100, 50);

        let root_win = root.root();
        assert_eq!(root_win.borrow().state.cols, 100);
        assert_eq!(root_win.borrow().state.rows, 50);
        assert_eq!(root_win.borrow().old.cols, 100);
        assert_eq!(root_win.borrow().old.rows, 50);

        let help_bar = root.help_bar();
        assert_eq!(help_bar.borrow().state.cols, 100);
    }

    #[test]
    fn merged_over_uses_base_when_overlay_unset() {
        let base = AttrColor::new(Some(Color::Blue), None, &[Attribute::Bold]);
        let overlay = AttrColor::unset();
        assert_eq!(AttrColor::merged_over(&base, &overlay), base);
    }

    #[test]
    fn simple_color_set_and_reset() {
        let mut ctx = GuiContext::new();
        let custom = AttrColor::new(Some(Color::Red), Some(Color::Blue), &[Attribute::Bold]);
        ctx.simple_color_set(ColorId::Status, custom.clone());
        assert_eq!(ctx.simple_color_get(ColorId::Status), custom);

        ctx.simple_color_reset(ColorId::Status);
        let expected = AttrColor::new(None, None, &[Attribute::Reverse]);
        assert_eq!(ctx.simple_color_get(ColorId::Status), expected);
    }

    #[test]
    fn merged_over_uses_explicit_overlay_fields() {
        let base = AttrColor::new(Some(Color::Blue), Some(Color::White), &[Attribute::Bold]);
        let overlay = AttrColor::new(None, Some(Color::Green), &[Attribute::Italic]);
        let merged = AttrColor::merged_over(&base, &overlay);

        assert_eq!(merged.fg, Color::Blue);
        assert_eq!(merged.bg, Color::Green);
        assert!(merged.attrs.contains(&Attribute::Bold));
        assert!(merged.attrs.contains(&Attribute::Italic));
        assert_eq!(merged.attrs.len(), 2);
    }

    #[test]
    fn simple_color_apply_config_updates_store() {
        let mut ctx = GuiContext::new();
        let entries = vec![
            ColorConfigEntry {
                cid: ColorId::Message,
                fg: Some(Color::Green),
                bg: None,
                attrs: vec![Attribute::Bold],
            },
            ColorConfigEntry {
                cid: ColorId::Status,
                fg: Some(Color::White),
                bg: Some(Color::Blue),
                attrs: Vec::new(),
            },
        ];

        simple_color_apply_config(&mut ctx, &entries);

        let msg_color = ctx.simple_color_get(ColorId::Message);
        assert_eq!(msg_color.fg, Color::Green);
        assert!(msg_color.attrs.contains(&Attribute::Bold));

        let status_color = ctx.simple_color_get(ColorId::Status);
        assert_eq!(status_color.fg, Color::White);
        assert_eq!(status_color.bg, Color::Blue);
    }

    #[test]
    fn merged_over_only_fg_or_bg() {
        let base = AttrColor::new(Some(Color::Red), Some(Color::Blue), &[Attribute::Bold]);
        let overlay = AttrColor::new(Some(Color::Green), None, &[]);
        let merged = AttrColor::merged_over(&base, &overlay);
        assert_eq!(merged.fg, Color::Green);
        assert_eq!(merged.bg, Color::Blue);
        assert!(merged.fg_set);
        assert!(merged.bg_set);

        let overlay = AttrColor::new(None, Some(Color::Yellow), &[]);
        let merged = AttrColor::merged_over(&base, &overlay);
        assert_eq!(merged.fg, Color::Red);
        assert_eq!(merged.bg, Color::Yellow);
        assert!(merged.fg_set);
        assert!(merged.bg_set);
    }

    #[test]
    fn merged_over_deduplicates_attrs() {
        let base = AttrColor::new(None, None, &[Attribute::Bold, Attribute::Italic]);
        let overlay = AttrColor::new(None, None, &[Attribute::Bold, Attribute::Underlined]);
        let merged = AttrColor::merged_over(&base, &overlay);
        let mut attrs = merged.attrs.clone();
        attrs.sort_by_key(|a| format!("{:?}", a));
        attrs.dedup();
        assert_eq!(attrs.len(), merged.attrs.len());
    }

    #[test]
    fn color_config_entry_to_attr_color_variants() {
        let entry = ColorConfigEntry {
            cid: ColorId::Message,
            fg: None,
            bg: None,
            attrs: Vec::new(),
        };
        let color = entry.to_attr_color();
        assert!(!color.is_set);

        let entry = ColorConfigEntry {
            cid: ColorId::Message,
            fg: Some(Color::Red),
            bg: Some(Color::Blue),
            attrs: vec![Attribute::Bold, Attribute::Italic],
        };
        let color = entry.to_attr_color();
        assert!(color.is_set);
        assert!(color.fg_set);
        assert!(color.bg_set);
        assert_eq!(color.attrs.len(), 2);
    }

    #[test]
    fn attr_color_apply_writes_escape() {
        let color = AttrColor::new(Some(Color::Red), Some(Color::Blue), &[Attribute::Bold]);
        let mut out = Vec::new();
        color.apply(&mut out).unwrap();
        assert!(out.contains(&b'\x1b'));
    }

    #[test]
    fn mutt_curses_set_color_updates_context_and_writes() {
        let mut ctx = GuiContext::new();
        let color = AttrColor::new(Some(Color::Green), None, &[Attribute::Italic]);
        let mut out = Vec::new();
        mutt_curses_set_color(&mut ctx, &mut out, &color).unwrap();
        assert_eq!(ctx.current_color(), &color);
        assert!(out.contains(&b'\x1b'));
    }

    #[test]
    fn mutt_curses_set_cursor_writes() {
        let mut ctx = GuiContext::new();
        let mut out = Vec::new();
        let old = mutt_curses_set_cursor(&mut ctx, &mut out, CursorState::Invisible).unwrap();
        assert_eq!(old, CursorState::Visible);
        assert_eq!(ctx.saved_cursor(), CursorState::Invisible);
        assert!(out.contains(&b'\x1b'));
    }

    #[test]
    fn simple_color_apply_config_empty_is_noop() {
        let mut ctx = GuiContext::new();
        let before = ctx.simple_color_get(ColorId::Status);
        simple_color_apply_config(&mut ctx, &[]);
        assert_eq!(ctx.simple_color_get(ColorId::Status), before);
    }
}
