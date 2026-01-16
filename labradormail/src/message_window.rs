//! Message window implementation.
//!
//! The message window displays status messages and prompts at the bottom
//! of the screen. It supports multi-line messages and colored text.

use std::io::Result;
use std::io::Write;

use crate::context::AttrColor;
use crate::context::ColorId;
use crate::context::GuiContext;
use crate::curs_lib::char_width;
use crate::render::Renderer;
use crate::window::handle_observer_delete;
use crate::window::CursorBehavior;
use crate::window::CursorState;
use crate::window::EventWindow;
use crate::window::HasObserverId;
use crate::window::NotifyWindow;
use crate::window::RenderMode;
use crate::window::WindowId;
use crate::window::WindowNotifyFlags;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowTree;
use crate::window::WindowType;
use crate::window::WindowWidget;

/// Maximum number of rows for message window.
pub const MSGWIN_MAX_ROWS: usize = 3;

/// Description of a single character.
#[derive(Debug, Clone, Default)]
pub struct MwChar {
    /// Width in screen cells.
    pub width: u8,
    /// Number of bytes to represent.
    pub bytes: u8,
    /// Color to use.
    pub color: AttrColor,
}

impl MwChar {
    /// Measures a character and returns its display width and byte length.
    ///
    /// This uses wcwidth-style semantics:
    /// - Control characters have width 0
    /// - ASCII characters have width 1
    /// - Wide characters (CJK, emoji) have width 2
    /// - Variation selectors have width 0
    pub fn measure(ch: char, color: AttrColor) -> Self {
        let width = char_width(ch) as u8;
        Self {
            width,
            bytes: ch.len_utf8() as u8,
            color,
        }
    }
}

/// A block of characters of one color.
#[derive(Debug, Clone, Default)]
pub struct MwChunk {
    /// Offset into text buffer.
    pub offset: u16,
    /// Number of bytes in the chunk.
    pub bytes: u16,
    /// Width in screen cells.
    pub width: u16,
    /// Color to use.
    pub color: AttrColor,
}

/// Message window private data.
#[derive(Debug, Clone, Default)]
pub struct MsgWinWindowData {
    /// Cached display string.
    pub text: String,
    /// Character breakdown.
    pub chars: Vec<MwChar>,
    /// Chunks per row.
    pub rows: [Vec<MwChunk>; MSGWIN_MAX_ROWS],
    /// Cursor row.
    pub row: i32,
    /// Cursor column.
    pub col: i32,
    /// Whether this message window handles cursor positioning.
    pub interactive: bool,
    /// Observer ID for window events.
    pub window_observer_id: Option<u64>,
}

impl HasObserverId for MsgWinWindowData {
    fn take_observer_id(&mut self) -> Option<u64> {
        self.window_observer_id.take()
    }
}

impl MsgWinWindowData {
    pub fn update(&mut self, tree: &mut WindowTree, win: WindowId) {
        let win_width = tree.get(win).state.rect.size.cols;
        let needed_rows = self.calc_rows(win_width).clamp(1, MSGWIN_MAX_ROWS as i16);

        let win_ref = tree.get_mut(win);
        let current_cols = win_ref.req_size.cols;
        win_ref.set_req_size(current_cols, needed_rows);
        win_ref.mark_repaint();
    }

    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        if matches!(mode, RenderMode::Paint) {
            let text = self.text.clone();
            let rows = self.rows.clone();
            let window = tree.get(win);
            let mut renderer = Renderer::new(ctx, out);

            let mut painted = false;
            for (row_idx, row_chunks) in rows.iter().enumerate() {
                if row_chunks.is_empty() {
                    continue;
                }
                painted = true;
                renderer.move_cursor(window, row_idx as i16, 0)?;
                for chunk in row_chunks {
                    let start = chunk.offset as usize;
                    let end = start + chunk.bytes as usize;
                    let slice = text.get(start..end).unwrap_or("");
                    renderer.set_color(&chunk.color)?;
                    renderer.write_str(slice)?;
                }
                renderer.set_color_by_id(ColorId::Normal)?;
                renderer.clrtoeol()?;
            }

            if !painted {
                renderer.set_color_by_id(ColorId::Normal)?;
                let rows = window.state.rect.size.rows;
                for row in 0..rows {
                    renderer.move_cursor(window, row, 0)?;
                    renderer.clrtoeol()?;
                }
            }
        }

        // Calculate cursor position after text is drawn.
        let mut final_row = 0i32;
        let mut final_col = 0i32;
        for (row_idx, row_chunks) in self.rows.iter().enumerate() {
            if row_chunks.is_empty() {
                break;
            }
            final_row = row_idx as i32;
            final_col = row_chunks.iter().map(|c| c.width as i32).sum();
        }
        self.row = final_row;
        self.col = final_col;

        if !self.interactive {
            return Ok(CursorBehavior::Hidden);
        }
        Ok(CursorBehavior::Positioned {
            row: self.row as i16,
            col: self.col as i16,
            state: CursorState::Visible,
        })
    }
}

impl MsgWinWindowData {
    /// Creates new message window data.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the message text.
    pub fn clear(&mut self) {
        self.text.clear();
        self.chars.clear();
        for row in &mut self.rows {
            row.clear();
        }
        self.row = 0;
        self.col = 0;
    }

    /// Sets the message text with a single color ID.
    pub fn set_text(&mut self, ctx: &GuiContext, text: &str, color: ColorId) {
        let normal = ctx.simple_color_get(ColorId::Normal);
        let overlay = ctx.simple_color_get(color);
        let merged = AttrColor::merged_over(&normal, &overlay);
        self.set_text_with_color(text, merged);
    }

    /// Sets the message text with a single color.
    pub fn set_text_with_color(&mut self, text: &str, color: AttrColor) {
        self.clear();
        self.add_text(text, color);
    }

    /// Appends text with the specified color.
    pub fn add_text(&mut self, text: &str, color: AttrColor) {
        self.text.push_str(text);

        // Calculate character widths using proper measurement
        for ch in text.chars() {
            if ch == '\u{FE0F}' {
                if let Some(prev) = self.chars.last_mut() {
                    if prev.width == 1 {
                        prev.width = 2;
                    }
                }
            }
            self.chars.push(MwChar::measure(ch, color.clone()));
        }
    }

    /// Calculates row chunks for wrapping text to a given width.
    ///
    /// This implements the msgwin_calc_rows logic from NeoMutt.
    /// Returns the number of rows needed (clamped to MSGWIN_MAX_ROWS).
    pub fn calc_rows(&mut self, win_width: i16) -> i16 {
        // Clear existing row data
        for row in &mut self.rows {
            row.clear();
        }

        if self.text.is_empty() || win_width <= 0 {
            return 0;
        }

        let mut width = 0i16;
        let mut offset: usize = 0;
        let mut row = 0usize;
        let mut new_row = false;
        let bytes = self.text.as_bytes();
        let mut chunk: Option<usize> = None;

        for mw_char in &self.chars {
            let is_newline = mw_char.bytes == 1 && bytes.get(offset) == Some(&b'\n');
            if is_newline {
                new_row = true;
                offset += mw_char.bytes as usize;
                continue;
            }

            if ((width + mw_char.width as i16) > win_width) || new_row {
                new_row = false;
                row += 1;
                if row >= MSGWIN_MAX_ROWS {
                    break;
                }

                self.rows[row].push(MwChunk {
                    offset: offset as u16,
                    bytes: mw_char.bytes as u16,
                    width: mw_char.width as u16,
                    color: mw_char.color.clone(),
                });
                chunk = Some(self.rows[row].len() - 1);
                width = 0;
            } else if chunk
                .and_then(|chunk_idx| self.rows[row].get(chunk_idx))
                .is_none_or(|row_chunk| row_chunk.color != mw_char.color)
            {
                self.rows[row].push(MwChunk {
                    offset: offset as u16,
                    bytes: mw_char.bytes as u16,
                    width: mw_char.width as u16,
                    color: mw_char.color.clone(),
                });
                chunk = Some(self.rows[row].len() - 1);
            } else if let Some(chunk_idx) = chunk {
                let row_chunk = &mut self.rows[row][chunk_idx];
                row_chunk.bytes += mw_char.bytes as u16;
                row_chunk.width += mw_char.width as u16;
            }

            offset += mw_char.bytes as usize;
            width += mw_char.width as i16;
        }

        ((row + 1) as i16).min(MSGWIN_MAX_ROWS as i16)
    }

    /// Gets the current text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns true if the message is empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Returns the total display width of the text.
    pub fn total_width(&self) -> u16 {
        self.chars.iter().map(|c| c.width as u16).sum()
    }
}

/// Message window wrapper.
///
/// Provides a high-level interface for the message window.
pub struct MessageWindow {
    window: WindowId,
    /// Whether this message window is interactive (active) or passive.
    /// Active windows handle their own drawing, passive windows don't.
    interactive: bool,
}

impl MessageWindow {
    /// Creates a new message window.
    ///
    /// If `interactive` is true, the window is active and handles its own drawing.
    /// If false, it's passive and drawing is handled externally.
    pub fn new(tree: &mut WindowTree, interactive: bool) -> Self {
        let rows = 1;

        let window = tree.add_window(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            rows,
        );

        {
            let borrowed = tree.get_mut(window);
            let mut widget_data = MsgWinWindowData::new();
            widget_data.interactive = interactive;
            borrowed.set_widget(WindowWidget::MessageWindow(widget_data));
        }
        let observer_id = tree.get_mut(window).add_observer(msgwin_window_observer);
        {
            let borrowed = tree.get_mut(window);
            if let Some(WindowWidget::MessageWindow(data)) = borrowed.widget_mut() {
                data.window_observer_id = Some(observer_id);
            }
        }

        Self {
            window,
            interactive,
        }
    }

    /// Returns the underlying window ID.
    pub fn window_id(&self) -> WindowId {
        self.window
    }

    /// Returns whether this message window is interactive (active).
    pub fn is_interactive(&self) -> bool {
        self.interactive
    }

    /// Sets the message text.
    pub fn set_text(&self, tree: &mut WindowTree, ctx: &GuiContext, text: &str, color: ColorId) {
        let borrowed = tree.get_mut(self.window);
        if let Some(WindowWidget::MessageWindow(data)) = borrowed.widget_mut() {
            data.set_text(ctx, text, color);
        }
        borrowed.mark_recalc_repaint();
    }

    /// Sets the message text with an explicit color.
    pub fn set_text_with_color(&self, tree: &mut WindowTree, text: &str, color: AttrColor) {
        let borrowed = tree.get_mut(self.window);
        if let Some(WindowWidget::MessageWindow(data)) = borrowed.widget_mut() {
            data.set_text_with_color(text, color);
        }
        borrowed.mark_recalc_repaint();
    }

    /// Adds text to the message.
    pub fn add_text(&self, tree: &mut WindowTree, text: &str, color: AttrColor) {
        let borrowed = tree.get_mut(self.window);
        if let Some(WindowWidget::MessageWindow(data)) = borrowed.widget_mut() {
            data.add_text(text, color);
        }
        borrowed.mark_recalc_repaint();
    }

    /// Clears the message text.
    pub fn clear_text(&self, tree: &mut WindowTree) {
        let borrowed = tree.get_mut(self.window);
        if let Some(WindowWidget::MessageWindow(data)) = borrowed.widget_mut() {
            data.clear();
        }
        borrowed.mark_recalc_repaint();
    }

    /// Gets the current message text.
    pub fn text(&self, tree: &WindowTree) -> Option<String> {
        if let Some(WindowWidget::MessageWindow(data)) = tree.get(self.window).widget_ref() {
            Some(data.text.clone())
        } else {
            None
        }
    }

    /// Sets the number of rows for the message window.
    pub fn set_rows(&self, tree: &mut WindowTree, rows: i16) {
        let win = tree.get_mut(self.window);
        let clamped = rows.clamp(1, MSGWIN_MAX_ROWS as i16);
        let current_cols = win.req_size.cols;
        win.set_req_size(current_cols, clamped);
    }

    /// Gets the message window data.
    pub fn data(&self, tree: &WindowTree) -> Option<MsgWinWindowData> {
        if let Some(WindowWidget::MessageWindow(data)) = tree.get(self.window).widget_ref() {
            Some(data.clone())
        } else {
            None
        }
    }

    /// Sets the cursor position within the message window.
    pub fn set_cursor(&self, tree: &mut WindowTree, row: i32, col: i32) {
        let win = tree.get_mut(self.window);
        if let Some(WindowWidget::MessageWindow(data)) = win.widget_mut() {
            data.row = row;
            data.col = col;
        }
    }
}

fn msgwin_window_observer(notify_type: NotifyWindow, event: &EventWindow, tree: &mut WindowTree) {
    match notify_type {
        NotifyWindow::State => {
            let flags = event.flags;
            if flags.contains(WindowNotifyFlags::HIDDEN) {
                if let Some(WindowWidget::MessageWindow(data)) =
                    tree.get_mut(event.win).widget_mut()
                {
                    data.clear();
                }
            }

            if flags.contains(WindowNotifyFlags::WIDER)
                || flags.contains(WindowNotifyFlags::NARROWER)
            {
                let cols = tree.get(event.win).state.rect.size.cols;
                let clamped = {
                    let Some(WindowWidget::MessageWindow(data)) =
                        tree.get_mut(event.win).widget_mut()
                    else {
                        return;
                    };
                    data.calc_rows(cols).clamp(1, MSGWIN_MAX_ROWS as i16)
                };
                let win_ref = tree.get_mut(event.win);
                let current_cols = win_ref.req_size.cols;
                win_ref.set_req_size(current_cols, clamped);
                win_ref.mark_recalc();
            } else {
                tree.get_mut(event.win).mark_repaint();
            }
        }
        NotifyWindow::Delete => {
            handle_observer_delete(tree, event.win);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::WindowActionFlags;
    use crossterm::style::Color;

    fn ac(color: Color) -> AttrColor {
        AttrColor::new(Some(color), None, &[])
    }

    #[test]
    fn msgwin_data_set_text() {
        let mut data = MsgWinWindowData::new();
        data.set_text_with_color("Hello, world!", ac(Color::White));

        assert_eq!(data.text(), "Hello, world!");
        assert!(!data.is_empty());
    }

    #[test]
    fn msgwin_data_add_text() {
        let mut data = MsgWinWindowData::new();
        data.add_text("Hello", ac(Color::Green));
        data.add_text(", ", ac(Color::White));
        data.add_text("world!", ac(Color::Blue));

        assert_eq!(data.text(), "Hello, world!");
    }

    #[test]
    fn msgwin_data_clear() {
        let mut data = MsgWinWindowData::new();
        data.set_text_with_color("Hello", ac(Color::White));
        assert!(!data.is_empty());

        data.clear();
        assert!(data.is_empty());
    }

    #[test]
    fn message_window_create() {
        let mut tree = WindowTree::new();
        let msgwin = MessageWindow::new(&mut tree, true);
        assert_eq!(tree.get(msgwin.window_id()).req_size.rows, 1);
        assert_eq!(
            tree.get(msgwin.window_id()).window_type,
            WindowType::Message
        );
    }

    #[test]
    fn message_window_set_text() {
        let ctx = GuiContext::new();
        let mut tree = WindowTree::new();
        let msgwin = MessageWindow::new(&mut tree, true);
        msgwin.set_text(&mut tree, &ctx, "Test message", ColorId::Message);

        let text = msgwin.text(&tree);
        assert!(text.is_some());
        assert_eq!(text.unwrap(), "Test message");
    }

    #[test]
    fn message_window_set_rows() {
        let mut tree = WindowTree::new();
        let msgwin = MessageWindow::new(&mut tree, true);
        assert_eq!(tree.get(msgwin.window_id()).req_size.rows, 1);

        msgwin.set_rows(&mut tree, 3);
        assert_eq!(tree.get(msgwin.window_id()).req_size.rows, 3);
    }

    #[test]
    fn mwchar_measure_ascii() {
        let ch = MwChar::measure('a', ac(Color::White));
        assert_eq!(ch.width, 1);
        assert_eq!(ch.bytes, 1);
    }

    #[test]
    fn mwchar_measure_control() {
        let ch = MwChar::measure('\n', ac(Color::White));
        assert_eq!(ch.width, 0);
        assert_eq!(ch.bytes, 1);
    }

    #[test]
    fn mwchar_measure_wide_char() {
        // CJK character (typically width 2)
        let ch = MwChar::measure('\u{4E2D}', ac(Color::White)); // Chinese "middle"
        assert_eq!(ch.width, 2);
        assert_eq!(ch.bytes, 3); // UTF-8 encoding
    }

    #[test]
    fn msgwin_calc_rows_single_line() {
        let mut data = MsgWinWindowData::new();
        data.set_text_with_color("Hello", ac(Color::White));

        let rows = data.calc_rows(80);
        assert_eq!(rows, 1);
        assert_eq!(data.rows[0].len(), 1);
        assert_eq!(data.rows[0][0].width, 5);
    }

    #[test]
    fn msgwin_calc_rows_wrap() {
        let mut data = MsgWinWindowData::new();
        // 10 characters, should wrap at width 5
        data.set_text_with_color("1234567890", ac(Color::White));

        let rows = data.calc_rows(5);
        assert_eq!(rows, 2, "10 chars at width 5 should wrap to 2 rows");
        assert_eq!(data.rows[0].len(), 1);
        assert_eq!(data.rows[0][0].width, 5);
        assert_eq!(data.rows[1].len(), 1);
        assert_eq!(data.rows[1][0].width, 5);
    }

    #[test]
    fn msgwin_calc_rows_colored_segments() {
        let mut data = MsgWinWindowData::new();
        data.add_text("Red", ac(Color::Red));
        data.add_text("Blue", ac(Color::Blue));

        let rows = data.calc_rows(80);
        assert_eq!(rows, 1);

        // Should have 2 chunks due to color change
        assert_eq!(data.rows[0].len(), 2);
        assert_eq!(data.rows[0][0].color, ac(Color::Red));
        assert_eq!(data.rows[0][0].width, 3);
        assert_eq!(data.rows[0][1].color, ac(Color::Blue));
        assert_eq!(data.rows[0][1].width, 4);
    }

    #[test]
    fn msgwin_calc_rows_empty() {
        let mut data = MsgWinWindowData::new();
        let rows = data.calc_rows(80);
        assert_eq!(rows, 0);
    }

    #[test]
    fn msgwin_calc_rows_max_rows() {
        let mut data = MsgWinWindowData::new();
        // Create text that would require more than MSGWIN_MAX_ROWS
        let long_text = "a".repeat(MSGWIN_MAX_ROWS * 10 + 5);
        data.set_text_with_color(&long_text, ac(Color::White));

        let rows = data.calc_rows(10);
        assert_eq!(
            rows, MSGWIN_MAX_ROWS as i16,
            "should clamp to MSGWIN_MAX_ROWS"
        );
    }

    #[test]
    fn msgwin_calc_rows_newline() {
        let mut data = MsgWinWindowData::new();
        data.set_text_with_color("A\nB", ac(Color::White));

        let rows = data.calc_rows(80);
        assert_eq!(rows, 2);
        assert_eq!(data.rows[0][0].offset, 0);
        assert_eq!(data.rows[0][0].bytes, 1);
        assert_eq!(data.rows[1][0].offset, 2);
        assert_eq!(data.rows[1][0].bytes, 1);
    }

    #[test]
    fn msgwin_calc_rows_emoji_variation_selector() {
        let mut data = MsgWinWindowData::new();
        data.set_text_with_color("\u{2764}\u{FE0F}", ac(Color::White));
        assert_eq!(data.chars.len(), 2);
        assert_eq!(data.chars[0].width, 2);
        assert_eq!(data.chars[1].width, 0);
    }

    #[test]
    fn msgwin_add_text_variation_selector_promotes_width() {
        let mut data = MsgWinWindowData::new();
        data.add_text("\u{2764}\u{FE0F}", ac(Color::White));
        assert_eq!(data.chars.len(), 2);
        assert_eq!(data.chars[0].width, 2);
        assert_eq!(data.chars[1].width, 0);
    }

    #[test]
    fn msgwin_calc_rows_wide_char_wraps() {
        let mut data = MsgWinWindowData::new();
        data.set_text_with_color("a\u{4E2D}", ac(Color::White));
        let rows = data.calc_rows(1);
        assert_eq!(rows, 2);
    }

    #[test]
    fn msgwin_calc_rows_zero_width_characters() {
        let mut data = MsgWinWindowData::new();
        data.set_text_with_color("\u{200D}\u{200D}", ac(Color::White));
        let rows = data.calc_rows(1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn message_window_interactive_flag() {
        let mut tree = WindowTree::new();
        let active = MessageWindow::new(&mut tree, true);
        assert!(active.is_interactive());

        let passive = MessageWindow::new(&mut tree, false);
        assert!(!passive.is_interactive());
    }

    #[test]
    fn msgwin_total_width() {
        let mut data = MsgWinWindowData::new();
        data.set_text_with_color("Hello", ac(Color::White));
        assert_eq!(data.total_width(), 5);

        // Add wide character
        data.add_text("\u{4E2D}", ac(Color::White));
        assert_eq!(data.total_width(), 7); // 5 + 2
    }

    #[test]
    fn msgwin_observer_reflows_on_width_change() {
        let ctx = GuiContext::new();
        let mut tree = WindowTree::new();
        let msgwin = MessageWindow::new(&mut tree, true);
        msgwin.set_text(&mut tree, &ctx, "1234567", ColorId::Message);

        {
            let win = tree.get_mut(msgwin.window_id());
            win.state.rect.size.cols = 3;
            win.actions = WindowActionFlags::empty();
        }

        let event = EventWindow {
            win: msgwin.window_id(),
            flags: WindowNotifyFlags::WIDER,
        };
        msgwin_window_observer(NotifyWindow::State, &event, &mut tree);

        let win = tree.get(msgwin.window_id());
        assert!(win.actions.contains(WindowActionFlags::RECALC));
        assert!(win.actions.contains(WindowActionFlags::REFLOW));
        assert!(win.req_size.rows > 1);
    }

    #[test]
    fn msgwin_draw_outputs_text() {
        let mut ctx = GuiContext::new();
        let mut tree = WindowTree::new();
        let msgwin = MessageWindow::new(&mut tree, true);
        msgwin.set_text(&mut tree, &ctx, "Hello", ColorId::Message);
        {
            let win = tree.get_mut(msgwin.window_id());
            win.state.rect.size.cols = 10;
            win.state.visible = true;
            win.mark_recalc_repaint();
        }
        let mut out = Vec::new();
        // Use redraw to trigger recalc and draw through the widget path.
        tree.redraw(msgwin.window_id(), &mut ctx, &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Hello"), "output = {}", output);
    }
}
