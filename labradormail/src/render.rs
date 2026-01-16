//! Rendering helpers that operate on window rectangles and a GUI context.

use std::io::Result;
use std::io::Write;

use crossterm::cursor::MoveTo;
use crossterm::style::Print;
use crossterm::terminal::Clear;
use crossterm::terminal::ClearType;
use crossterm::ExecutableCommand;

use crate::context::AttrColor;
use crate::context::ColorId;
use crate::context::GuiContext;
use crate::curs_lib::pad_string;
use crate::window::Window;

/// Moves the cursor to a position within a window.
pub fn move_cursor(out: &mut dyn Write, win: &Window, row: i16, col: i16) -> Result<()> {
    let abs_row = win.state.rect.origin.row + row;
    let abs_col = win.state.rect.origin.col + col;
    if abs_row < 0 || abs_col < 0 {
        return Ok(());
    }
    out.execute(MoveTo(abs_col as u16, abs_row as u16))?;
    Ok(())
}

/// Rendering helper that owns the output sink and GUI context.
pub struct Renderer<'a> {
    ctx: &'a mut GuiContext,
    out: &'a mut dyn Write,
}

impl<'a> Renderer<'a> {
    /// Creates a new renderer for the given GUI context and output.
    pub fn new(ctx: &'a mut GuiContext, out: &'a mut dyn Write) -> Self {
        Self { ctx, out }
    }

    /// Moves the cursor to a position within a window.
    pub fn move_cursor(&mut self, win: &Window, row: i16, col: i16) -> Result<()> {
        move_cursor(self.out, win, row, col)
    }

    /// Writes a character at the current cursor position.
    pub fn write_char(&mut self, ch: char) -> Result<()> {
        self.out.execute(Print(ch))?;
        Ok(())
    }

    /// Writes a string at the current cursor position.
    pub fn write_str(&mut self, s: &str) -> Result<()> {
        self.out.execute(Print(s))?;
        Ok(())
    }

    /// Writes at most n characters of a string.
    pub fn addnstr(&mut self, s: &str, n: usize) -> Result<()> {
        let truncated: String = s.chars().take(n).collect();
        self.out.execute(Print(truncated))?;
        Ok(())
    }

    /// Writes a formatted string at the current cursor position.
    pub fn printf(&mut self, args: std::fmt::Arguments<'_>) -> Result<()> {
        self.out.execute(Print(args.to_string()))?;
        Ok(())
    }

    /// Sets the current color.
    pub fn set_color(&mut self, color: &AttrColor) -> Result<()> {
        self.ctx.set_color(self.out, color)
    }

    /// Sets the current color by ID.
    pub fn set_color_by_id(&mut self, color: ColorId) -> Result<()> {
        self.ctx.set_color_by_id(self.out, color).map(|_| ())
    }

    /// Writes a string padded to the window width at the current cursor position.
    pub fn write_padded(&mut self, win: &Window, s: &str) -> Result<()> {
        let width = win.state.rect.size.cols.max(0) as usize;
        let padded = pad_string(s, width);
        self.write_str(&padded)
    }

    /// Clears the entire window.
    pub fn clear(&mut self, win: &Window) -> Result<()> {
        if win.state.rect.size.rows <= 0 {
            return Ok(());
        }
        for row in 0..win.state.rect.size.rows {
            self.clearline(win, row)?;
        }
        Ok(())
    }

    /// Clears a single row within the window.
    pub fn clearline(&mut self, win: &Window, row: i16) -> Result<()> {
        if win.state.rect.size.cols <= 0 {
            return Ok(());
        }
        self.move_cursor(win, row, 0)?;
        let current = self.ctx.current_color().clone();
        self.ctx.set_color(self.out, &current)?;
        let spaces = " ".repeat(win.state.rect.size.cols as usize);
        self.out.execute(Print(spaces))?;
        Ok(())
    }

    /// Clears from cursor to end of line.
    pub fn clrtoeol(&mut self) -> Result<()> {
        let current = self.ctx.current_color().clone();
        self.ctx.set_color(self.out, &current)?;
        self.out.execute(Clear(ClearType::UntilNewLine))?;
        Ok(())
    }

    /// Draws a single-row bar (status bar, help bar, etc.) with the given text.
    pub fn draw_single_row_bar(&mut self, win: &Window, text: &str) -> Result<()> {
        self.move_cursor(win, 0, 0)?;
        let width = win.state.rect.size.cols.max(0) as usize;
        let padded = pad_string(text, width);
        self.write_str(&padded)
    }

    /// Draws a list with selection highlighting.
    pub fn draw_list_with_selection<F>(
        &mut self,
        win: &Window,
        scroll_offset: usize,
        mut get_item: F,
    ) -> Result<()>
    where
        F: FnMut(usize) -> Option<(String, bool)>,
    {
        let width = win.state.rect.size.cols.max(0) as usize;
        for row in 0i16..win.state.rect.size.rows {
            self.move_cursor(win, row, 0)?;
            let idx = scroll_offset + row as usize;

            match get_item(idx) {
                Some((text, is_selected)) => {
                    if is_selected {
                        self.ctx.set_color_by_id(self.out, ColorId::Indicator)?;
                    } else {
                        self.ctx.set_color_by_id(self.out, ColorId::Normal)?;
                    }
                    let padded = pad_string(&text, width);
                    self.write_str(&padded)?;
                    self.ctx.set_color_by_id(self.out, ColorId::Normal)?;
                }
                None => {
                    self.ctx.set_color_by_id(self.out, ColorId::Normal)?;
                    let padded = pad_string("", width);
                    self.write_str(&padded)?;
                }
            }
        }
        Ok(())
    }
}
