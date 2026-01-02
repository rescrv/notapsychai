//! Message window implementation.
//!
//! The message window displays status messages and prompts at the bottom
//! of the screen. It supports multi-line messages and colored text.

use std::cell::RefCell;
use std::io::Result;
use std::io::Write;
use std::rc::Rc;

use crate::context::GuiContext;
use crate::curs_lib::mutt_char_width;
use crate::mutt_curses::mutt_curses_set_color;
use crate::mutt_curses::mutt_curses_set_color_by_id;
use crate::mutt_curses::mutt_curses_set_cursor;
use crate::mutt_curses::AttrColor;
use crate::mutt_curses::ColorId;
use crate::window::handle_observer_delete;
use crate::window::CursorState;
use crate::window::EventWindow;
use crate::window::HasObserverId;
use crate::window::MuttWindow;
use crate::window::NotifyWindow;
use crate::window::WindowActionFlags;
use crate::window::WindowNotifyFlags;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowType;
use crate::MSGWIN_MAX_ROWS;

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
        let width = mutt_char_width(ch) as u8;
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
    /// Observer ID for window events.
    pub window_observer_id: Option<u64>,
}

impl HasObserverId for MsgWinWindowData {
    fn take_observer_id(&mut self) -> Option<u64> {
        self.window_observer_id.take()
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
            } else if chunk.is_none() || self.rows[row][chunk.unwrap()].color != mw_char.color {
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
    window: Rc<RefCell<MuttWindow>>,
    /// Whether this message window is interactive (active) or passive.
    /// Active windows handle their own drawing, passive windows don't.
    interactive: bool,
}

impl MessageWindow {
    /// Creates a new message window.
    ///
    /// If `interactive` is true, the window is active and handles its own drawing.
    /// If false, it's passive and drawing is handled externally.
    pub fn new(interactive: bool) -> Self {
        let rows = 1;

        let window = MuttWindow::new(
            WindowType::Message,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            rows,
        );

        {
            let mut borrowed = window.borrow_mut();
            borrowed.wdata = Some(Box::new(MsgWinWindowData::new()));
            borrowed.recalc = Some(msgwin_recalc);
            borrowed.repaint = Some(msgwin_repaint);
            borrowed.draw = Some(msgwin_draw);
            if interactive {
                borrowed.recursor = Some(msgwin_recursor);
            }
        }
        let observer_id = window.borrow_mut().add_observer(msgwin_window_observer);
        {
            let mut borrowed = window.borrow_mut();
            if let Some(data) = borrowed.wdata_mut::<MsgWinWindowData>() {
                data.window_observer_id = Some(observer_id);
            }
        }

        Self {
            window,
            interactive,
        }
    }

    /// Returns a reference to the underlying window.
    pub fn window(&self) -> &Rc<RefCell<MuttWindow>> {
        &self.window
    }

    /// Returns whether this message window is interactive (active).
    pub fn is_interactive(&self) -> bool {
        self.interactive
    }

    /// Sets the message text.
    pub fn set_text(&self, ctx: &GuiContext, text: &str, color: ColorId) {
        let mut borrowed = self.window.borrow_mut();
        if let Some(data) = borrowed.wdata_mut::<MsgWinWindowData>() {
            data.set_text(ctx, text, color);
        }
        borrowed.actions |= WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
    }

    /// Sets the message text with an explicit color.
    pub fn set_text_with_color(&self, text: &str, color: AttrColor) {
        let mut borrowed = self.window.borrow_mut();
        if let Some(data) = borrowed.wdata_mut::<MsgWinWindowData>() {
            data.set_text_with_color(text, color);
        }
        borrowed.actions |= WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
    }

    /// Adds text to the message.
    pub fn add_text(&self, text: &str, color: AttrColor) {
        let mut borrowed = self.window.borrow_mut();
        if let Some(data) = borrowed.wdata_mut::<MsgWinWindowData>() {
            data.add_text(text, color);
        }
        borrowed.actions |= WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
    }

    /// Clears the message text.
    pub fn clear_text(&self) {
        let mut borrowed = self.window.borrow_mut();
        if let Some(data) = borrowed.wdata_mut::<MsgWinWindowData>() {
            data.clear();
        }
        borrowed.actions |= WindowActionFlags::RECALC | WindowActionFlags::REPAINT;
    }

    /// Gets the current message text.
    pub fn text(&self) -> Option<String> {
        let borrowed = self.window.borrow();
        borrowed
            .wdata_ref::<MsgWinWindowData>()
            .map(|data| data.text.clone())
    }

    /// Sets the number of rows for the message window.
    pub fn set_rows(&self, rows: i16) {
        let mut borrowed = self.window.borrow_mut();
        let clamped = rows.clamp(1, MSGWIN_MAX_ROWS as i16);
        if borrowed.req_rows != clamped {
            borrowed.req_rows = clamped;
            borrowed.actions |= WindowActionFlags::REFLOW;
        }
    }

    /// Gets the message window data.
    pub fn data(&self) -> Option<MsgWinWindowData> {
        let borrowed = self.window.borrow();
        borrowed.wdata_ref::<MsgWinWindowData>().cloned()
    }

    /// Sets the cursor position within the message window.
    pub fn set_cursor(&self, row: i32, col: i32) {
        let mut borrowed = self.window.borrow_mut();
        if let Some(data) = borrowed.wdata_mut::<MsgWinWindowData>() {
            data.row = row;
            data.col = col;
        }
    }
}

/// Recalculate callback for message window.
///
/// Following NeoMutt's pattern: recalc requests repaint and calculates row layout.
fn msgwin_recalc(win: &mut MuttWindow) {
    let win_width = win.state.cols;

    if let Some(data) = win.wdata_mut::<MsgWinWindowData>() {
        let needed_rows = data.calc_rows(win_width).clamp(1, MSGWIN_MAX_ROWS as i16);

        // Update req_rows if changed (to trigger reflow if needed)
        if win.req_rows != needed_rows {
            win.req_rows = needed_rows;
            win.actions |= WindowActionFlags::REFLOW;
        }
    }

    win.actions |= WindowActionFlags::REPAINT;
}

/// Repaint callback for message window.
///
/// In NeoMutt, this function draws directly using curses functions.
/// In our crossterm implementation, the MsgWinWindowData contains all
/// information needed to render (text, rows with chunks, cursor position).
///
/// The actual terminal output happens in the draw phase which has write access.
/// This callback computes the final cursor position after drawing would complete.
fn msgwin_repaint(win: &mut MuttWindow) {
    if let Some(data) = win.wdata_mut::<MsgWinWindowData>() {
        // Calculate cursor position after text is drawn
        // This mimics NeoMutt's mutt_window_get_coords at end of repaint
        let mut final_row = 0i32;
        let mut final_col = 0i32;

        for (row_idx, row_chunks) in data.rows.iter().enumerate() {
            if row_chunks.is_empty() {
                break;
            }
            final_row = row_idx as i32;
            final_col = row_chunks.iter().map(|c| c.width as i32).sum();
        }

        data.row = final_row;
        data.col = final_col;
    }
}

fn msgwin_draw(win: &mut MuttWindow, ctx: &mut GuiContext, out: &mut dyn Write) -> Result<()> {
    let Some(data) = win.wdata_ref::<MsgWinWindowData>() else {
        return Ok(());
    };
    let text = data.text.clone();
    let rows = data.rows.clone();

    let mut painted = false;
    for (row_idx, row_chunks) in rows.iter().enumerate() {
        if row_chunks.is_empty() {
            continue;
        }
        painted = true;
        win.move_cursor(out, row_idx as i16, 0)?;
        for chunk in row_chunks {
            let start = chunk.offset as usize;
            let end = start + chunk.bytes as usize;
            let slice = text.get(start..end).unwrap_or("");
            mutt_curses_set_color(ctx, out, &chunk.color)?;
            win.addstr(out, slice)?;
        }
        mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
        win.clrtoeol(ctx, out)?;
    }

    if !painted {
        mutt_curses_set_color_by_id(ctx, out, ColorId::Normal)?;
        for row in 0..win.state.rows {
            win.move_cursor(out, row, 0)?;
            win.clrtoeol(ctx, out)?;
        }
    }

    Ok(())
}

fn msgwin_recursor(win: &mut MuttWindow, ctx: &mut GuiContext, out: &mut dyn Write) -> bool {
    let Some(data) = win.wdata_ref::<MsgWinWindowData>() else {
        return false;
    };
    let row = data.row;
    let col = data.col;

    if win.move_cursor(out, row as i16, col as i16).is_err() {
        return false;
    }
    let _ = mutt_curses_set_cursor(ctx, out, CursorState::Visible);
    true
}

fn msgwin_window_observer(notify_type: NotifyWindow, event: &EventWindow) {
    let win = match event.win.upgrade() {
        Some(win) => win,
        None => return,
    };

    match notify_type {
        NotifyWindow::State => {
            let flags = event.flags;
            if flags.contains(WindowNotifyFlags::HIDDEN) {
                if let Some(data) = win.borrow_mut().wdata_mut::<MsgWinWindowData>() {
                    data.clear();
                }
            }

            if flags.contains(WindowNotifyFlags::WIDER)
                || flags.contains(WindowNotifyFlags::NARROWER)
            {
                let mut borrowed = win.borrow_mut();
                let cols = borrowed.state.cols;
                if let Some(data) = borrowed.wdata_mut::<MsgWinWindowData>() {
                    let needed_rows = data.calc_rows(cols);
                    let clamped = needed_rows.clamp(1, MSGWIN_MAX_ROWS as i16);
                    if borrowed.req_rows != clamped {
                        borrowed.req_rows = clamped;
                        borrowed.actions |= WindowActionFlags::REFLOW;
                    }
                }
                borrowed.actions |= WindowActionFlags::RECALC;
            } else {
                win.borrow_mut().actions |= WindowActionFlags::REPAINT;
            }
        }
        NotifyWindow::Delete => {
            handle_observer_delete::<MsgWinWindowData>(&win);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let msgwin = MessageWindow::new(true);
        assert_eq!(msgwin.window().borrow().req_rows, 1);
        assert_eq!(msgwin.window().borrow().window_type, WindowType::Message);
    }

    #[test]
    fn message_window_set_text() {
        let ctx = GuiContext::new();
        let msgwin = MessageWindow::new(true);
        msgwin.set_text(&ctx, "Test message", ColorId::Message);

        let text = msgwin.text();
        assert!(text.is_some());
        assert_eq!(text.unwrap(), "Test message");
    }

    #[test]
    fn message_window_set_rows() {
        let msgwin = MessageWindow::new(true);
        assert_eq!(msgwin.window().borrow().req_rows, 1);

        msgwin.set_rows(3);
        assert_eq!(msgwin.window().borrow().req_rows, 3);
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
        let active = MessageWindow::new(true);
        assert!(active.is_interactive());

        let passive = MessageWindow::new(false);
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
        let msgwin = MessageWindow::new(true);
        msgwin.set_text(&ctx, "1234567", ColorId::Message);

        {
            let mut win = msgwin.window().borrow_mut();
            win.state.cols = 3;
            win.actions = WindowActionFlags::empty();
        }

        let event = EventWindow {
            win: Rc::downgrade(msgwin.window()),
            flags: WindowNotifyFlags::WIDER,
        };
        msgwin_window_observer(NotifyWindow::State, &event);

        let win = msgwin.window().borrow();
        assert!(win.actions.contains(WindowActionFlags::RECALC));
        assert!(win.actions.contains(WindowActionFlags::REFLOW));
        assert!(win.req_rows > 1);
    }

    #[test]
    fn msgwin_draw_outputs_text() {
        let mut ctx = GuiContext::new();
        let msgwin = MessageWindow::new(true);
        msgwin.set_text(&ctx, "Hello", ColorId::Message);
        {
            let mut win = msgwin.window().borrow_mut();
            win.state.cols = 10;
            msgwin_recalc(&mut win);
        }
        let mut out = Vec::new();
        let mut win = msgwin.window().borrow_mut();
        msgwin_draw(&mut win, &mut ctx, &mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Hello"));
    }
}
