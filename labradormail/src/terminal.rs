//! Terminal utilities.
//!
//! Provides functions for terminal title, icon, and resize handling.

use std::io::Result;
use std::io::Write;

use crossterm::cursor::SetCursorStyle;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use crossterm::terminal::size;
use crossterm::ExecutableCommand;

use crate::window::CursorState;

/// Terminal management utilities.
pub struct Terminal;

impl Terminal {
    /// Gets the current terminal size.
    pub fn size() -> Result<(u16, u16)> {
        size()
    }

    /// Sets the terminal title.
    pub fn set_title(out: &mut dyn Write, title: &str) -> Result<()> {
        out.execute(crossterm::terminal::SetTitle(title))?;
        Ok(())
    }

    /// Clears the terminal screen.
    pub fn clear(out: &mut dyn Write) -> Result<()> {
        out.execute(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::All,
        ))?;
        Ok(())
    }

    /// Moves cursor to position.
    pub fn move_to(out: &mut dyn Write, col: u16, row: u16) -> Result<()> {
        out.execute(crossterm::cursor::MoveTo(col, row))?;
        Ok(())
    }

    /// Shows the cursor.
    pub fn show_cursor(out: &mut dyn Write) -> Result<()> {
        out.execute(crossterm::cursor::Show)?;
        Ok(())
    }

    /// Hides the cursor.
    pub fn hide_cursor(out: &mut dyn Write) -> Result<()> {
        out.execute(crossterm::cursor::Hide)?;
        Ok(())
    }

    /// Flushes output.
    pub fn flush(out: &mut dyn Write) -> Result<()> {
        out.flush()
    }

    /// Checks if an event is a resize event.
    pub fn is_resize_event(event: &Event) -> bool {
        matches!(event, Event::Resize(_, _))
    }

    /// Extracts resize dimensions from a resize event.
    pub fn get_resize_dimensions(event: &Event) -> Option<(u16, u16)> {
        if let Event::Resize(cols, rows) = event {
            Some((*cols, *rows))
        } else {
            None
        }
    }

    /// Checks if a key event is Ctrl+C.
    pub fn is_ctrl_c(event: &KeyEvent) -> bool {
        event.code == KeyCode::Char('c') && event.modifiers.contains(KeyModifiers::CONTROL)
    }

    /// Checks if a key event is Escape.
    pub fn is_escape(event: &KeyEvent) -> bool {
        event.code == KeyCode::Esc
    }

    /// Checks if a key event is Enter.
    pub fn is_enter(event: &KeyEvent) -> bool {
        event.code == KeyCode::Enter
    }

    /// Beeps the terminal.
    pub fn beep(out: &mut dyn Write) -> Result<()> {
        write!(out, "\x07")?;
        out.flush()
    }

    /// Sets the cursor visibility state.
    ///
    /// Matches NeoMutt's cursor semantics:
    /// - Invisible: cursor is hidden
    /// - Visible: cursor is shown (default line cursor)
    /// - VeryVisible: cursor is shown as a block (more visible)
    pub fn set_cursor(out: &mut dyn Write, state: CursorState) -> Result<()> {
        match state {
            CursorState::Invisible => {
                out.execute(crossterm::cursor::Hide)?;
            }
            CursorState::Visible => {
                out.execute(crossterm::cursor::Show)?;
                out.execute(SetCursorStyle::DefaultUserShape)?;
            }
            CursorState::VeryVisible => {
                out.execute(crossterm::cursor::Show)?;
                out.execute(SetCursorStyle::SteadyBlock)?;
            }
        }
        Ok(())
    }

    /// Returns true if terminal supports the legacy "ts" capability.
    ///
    /// NeoMutt uses terminfo for this, but the Rust port currently does not
    /// depend on a terminfo parser, so this always returns false.
    pub fn ts_capability() -> bool {
        false
    }

    /// Sets the terminal status line (no-op without terminfo support).
    ///
    /// This remains a stub until terminfo capabilities are modeled in Rust.
    pub fn ts_status(_out: &mut dyn Write, _status: &str) -> Result<()> {
        Ok(())
    }

    /// Sets the terminal icon title (no-op without terminfo support).
    ///
    /// This remains a stub until terminfo capabilities are modeled in Rust.
    pub fn ts_icon(_out: &mut dyn Write, _icon: &str) -> Result<()> {
        Ok(())
    }
}

/// Returns true if terminal supports "ts" capability (stubbed false).
pub fn mutt_ts_capability() -> bool {
    Terminal::ts_capability()
}

/// Sets terminal status line (no-op without terminfo support).
pub fn mutt_ts_status(out: &mut dyn Write, status: &str) -> Result<()> {
    Terminal::ts_status(out, status)
}

/// Sets terminal icon title (no-op without terminfo support).
pub fn mutt_ts_icon(out: &mut dyn Write, icon: &str) -> Result<()> {
    Terminal::ts_icon(out, icon)
}

/// Resize event handler.
///
/// Tracks terminal resize events and provides the new dimensions.
#[derive(Debug, Default)]
pub struct ResizeHandler {
    last_cols: u16,
    last_rows: u16,
}

impl ResizeHandler {
    /// Creates a new resize handler.
    pub fn new() -> Result<Self> {
        let (cols, rows) = size()?;
        Ok(Self {
            last_cols: cols,
            last_rows: rows,
        })
    }

    /// Handles an event, returning new dimensions if resized.
    pub fn handle(&mut self, event: &Event) -> Option<(u16, u16)> {
        if let Event::Resize(cols, rows) = event {
            if *cols != self.last_cols || *rows != self.last_rows {
                self.last_cols = *cols;
                self.last_rows = *rows;
                return Some((*cols, *rows));
            }
        }
        None
    }

    /// Gets the last known terminal size.
    pub fn size(&self) -> (u16, u16) {
        (self.last_cols, self.last_rows)
    }

    /// Refreshes the size from the terminal.
    pub fn refresh(&mut self) -> Result<(u16, u16)> {
        let (cols, rows) = size()?;
        self.last_cols = cols;
        self.last_rows = rows;
        Ok((cols, rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_resize_event() {
        let resize = Event::Resize(80, 24);
        assert!(Terminal::is_resize_event(&resize));

        let key = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(!Terminal::is_resize_event(&key));
    }

    #[test]
    fn get_resize_dimensions() {
        let resize = Event::Resize(100, 50);
        let dims = Terminal::get_resize_dimensions(&resize);
        assert_eq!(dims, Some((100, 50)));

        let key = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let dims = Terminal::get_resize_dimensions(&key);
        assert_eq!(dims, None);
    }

    #[test]
    fn is_ctrl_c() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(Terminal::is_ctrl_c(&ctrl_c));

        let just_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(!Terminal::is_ctrl_c(&just_c));
    }

    #[test]
    fn is_escape() {
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(Terminal::is_escape(&esc));

        let other = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(!Terminal::is_escape(&other));
    }

    #[test]
    fn is_enter() {
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(Terminal::is_enter(&enter));
    }

    #[test]
    fn resize_handler_handle() {
        let mut handler = ResizeHandler {
            last_cols: 80,
            last_rows: 24,
        };

        // Same size - no change
        let same = Event::Resize(80, 24);
        assert!(handler.handle(&same).is_none());

        // Different size - returns new dimensions
        let different = Event::Resize(100, 50);
        let result = handler.handle(&different);
        assert_eq!(result, Some((100, 50)));
        assert_eq!(handler.size(), (100, 50));
    }

    #[test]
    fn set_title_writes_output() {
        let mut out = Vec::new();
        Terminal::set_title(&mut out, "NeoMutt").unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("NeoMutt"));
    }

    #[test]
    fn clear_writes_escape() {
        let mut out = Vec::new();
        Terminal::clear(&mut out).unwrap();
        assert!(out.contains(&b'\x1b'));
    }

    #[test]
    fn beep_writes_bell() {
        let mut out = Vec::new();
        Terminal::beep(&mut out).unwrap();
        assert_eq!(out, vec![0x07]);
    }

    #[test]
    fn set_cursor_writes_escape() {
        let mut out = Vec::new();
        Terminal::set_cursor(&mut out, CursorState::VeryVisible).unwrap();
        assert!(out.contains(&b'\x1b'));
    }
}
