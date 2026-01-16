//! Curses-style UI glue helpers.
//!
//! This module mirrors key behaviors from NeoMutt's curs_lib.c for width
//! handling, padding/truncation, and simple key prompts.

use std::io::Write;

use crossterm::event::read;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use crossterm::style::Print;
use crossterm::terminal::Clear;
use crossterm::terminal::ClearType;
use crossterm::ExecutableCommand;
use unicode_width::UnicodeWidthChar;

/// Measures the display width of a character.
///
/// Matches wcwidth-style semantics and treats control/zero-width characters
/// as occupying zero cells.
pub fn char_width(ch: char) -> usize {
    if ch.is_control() {
        return 0;
    }

    if is_variation_selector(ch) || is_zero_width(ch) || is_emoji_modifier(ch) || is_keycap(ch) {
        return 0;
    }

    UnicodeWidthChar::width(ch).unwrap_or(1)
}

/// Result of a string truncation operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Truncation {
    /// Number of bytes that fit.
    pub bytes: usize,
    /// Display width used.
    pub width: usize,
}

/// Truncate a string by max byte length and max display width.
///
/// Returns a `Truncation` struct containing the number of bytes that fit
/// and the resulting display width.
pub fn truncate_str_width(src: &str, maxlen: usize, maxwid: usize) -> Truncation {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut used_width = 0;
    let mut used_len = 0;
    let mut i = 0;

    while i < chars.len() {
        let (idx, ch) = chars[i];

        // Stop conditions
        if idx >= maxlen || idx + ch.len_utf8() > maxlen || ch == '\n' {
            break;
        }

        let (next_i, width) = cluster_width_at(&chars, i);

        // Calculate end byte of the full cluster
        let end_byte = if next_i < chars.len() {
            chars[next_i].0
        } else {
            src.len()
        };

        if end_byte > maxlen || used_width + width > maxwid {
            break;
        }

        used_width += width;
        used_len = end_byte;
        i = next_i;
    }

    Truncation {
        bytes: used_len,
        width: used_width,
    }
}

/// Pads or truncates a string to exactly `width` screen cells.
pub(crate) fn pad_string(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let trunc = truncate_str_width(s, s.len(), width);
    let mut out = s.get(..trunc.bytes).unwrap_or("").to_string();

    if trunc.width < width {
        out.extend(std::iter::repeat_n(' ', width - trunc.width));
    }

    out
}

/// Scroll state for managing viewport scrolling through a list of items.
///
/// Provides common scroll operations used by list views like help dialog
/// and message index.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollState {
    offset: usize,
}

impl ScrollState {
    /// Creates a new scroll state at offset 0.
    pub fn new() -> Self {
        Self { offset: 0 }
    }

    /// Returns the current scroll offset.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Scrolls up by the given amount, stopping at 0.
    pub fn scroll_up(&mut self, amount: usize) {
        self.offset = self.offset.saturating_sub(amount);
    }

    /// Scrolls down by the given amount, stopping at max_offset.
    pub fn scroll_down(&mut self, amount: usize, max_offset: usize) {
        self.offset = (self.offset + amount).min(max_offset);
    }

    /// Scrolls to show the bottom of the list.
    ///
    /// Sets offset so the last `visible_rows` items are shown.
    pub fn scroll_to_bottom(&mut self, visible_rows: usize, total_items: usize) {
        self.offset = total_items.saturating_sub(visible_rows);
    }

    /// Scrolls to the top of the list.
    pub fn scroll_to_top(&mut self) {
        self.offset = 0;
    }

    /// Calculates the maximum scroll offset for a list.
    ///
    /// Returns 0 if the list fits entirely in the viewport.
    pub fn max_offset(total_items: usize, visible_rows: usize) -> usize {
        total_items.saturating_sub(visible_rows)
    }
}

bitflags::bitflags! {
    /// Flags that control file selection behavior.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SelectFileFlags: u32 {
        /// No special behavior.
        const NONE = 0;
    }
}

/// State for file completion and multi-select prompts.
#[derive(Debug, Clone, Default)]
pub struct FileCompletionData {
    /// Allow multiple selections.
    pub multiple: bool,
    /// Use mailbox context when completing paths.
    pub mailbox: bool,
    /// List of selected files.
    pub files: Vec<String>,
    /// Number of selected files.
    pub numfiles: usize,
}

/// Convert tabs to spaces in a string.
pub fn expand_tabs(s: &str, len: usize, tabwidth: usize) -> Option<String> {
    if s.is_empty() || len == 0 || tabwidth < 1 {
        return None;
    }

    let mut out = String::with_capacity(len + len / 8);
    let mut width = 0usize;

    for (idx, ch) in s.char_indices() {
        if idx >= len || idx + ch.len_utf8() > len {
            break;
        }

        if ch == '\t' {
            let indent = tabwidth - (width % tabwidth);
            out.extend(std::iter::repeat_n(' ', indent));
            width += indent;
        } else {
            out.push(ch);
            width += char_width(ch);
        }
    }

    Some(out)
}

fn mw_enter_fname_with<R>(
    out: &mut dyn Write,
    prompt: &str,
    fname: &mut String,
    completion: &mut FileCompletionData,
    _flags: SelectFileFlags,
    mut read_event: R,
) -> std::io::Result<i32>
where
    R: FnMut() -> std::io::Result<Event>,
{
    out.execute(Print("\r"))?;
    out.execute(Clear(ClearType::CurrentLine))?;
    out.execute(Print(prompt))?;
    out.execute(Print(" "))?;
    out.execute(Print(fname.as_str()))?;
    out.flush()?;

    let mut input = fname.clone();

    loop {
        if let Event::Key(key) = read_event()? {
            match key.code {
                KeyCode::Enter => {
                    break;
                }
                KeyCode::Backspace => {
                    if input.pop().is_some() {
                        out.execute(Print("\x08 \x08"))?;
                    }
                }
                KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(-1);
                }
                KeyCode::Esc => {
                    return Ok(-1);
                }
                KeyCode::Char(ch) => {
                    input.push(ch);
                    out.execute(Print(ch))?;
                }
                _ => {}
            }
            out.flush()?;
        }
    }

    let trimmed = input.trim();
    *fname = trimmed.to_string();
    completion.files.clear();
    completion.numfiles = 0;
    if completion.multiple {
        completion.files = trimmed
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        completion.numfiles = completion.files.len();
    }

    Ok(0)
}

/// Prompt the user to enter a filename (basic line editor).
pub fn mw_enter_fname(
    out: &mut dyn Write,
    prompt: &str,
    fname: &mut String,
    completion: &mut FileCompletionData,
    _flags: SelectFileFlags,
) -> std::io::Result<i32> {
    mw_enter_fname_with(out, prompt, fname, completion, _flags, read)
}

fn is_variation_selector(ch: char) -> bool {
    matches!(ch, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

fn is_emoji_modifier(ch: char) -> bool {
    matches!(ch, '\u{1F3FB}'..='\u{1F3FF}')
}

fn is_regional_indicator(ch: char) -> bool {
    matches!(ch, '\u{1F1E6}'..='\u{1F1FF}')
}

fn is_keycap(ch: char) -> bool {
    ch == '\u{20E3}'
}

fn is_zero_width(ch: char) -> bool {
    matches!(
        ch,
        '\u{200B}' // Zero width space
        | '\u{200C}' // Zero width non-joiner
        | '\u{200D}' // Zero width joiner
        | '\u{2060}' // Word joiner
        | '\u{FEFF}' // Zero width no-break space
    )
}

fn cluster_width_at(chars: &[(usize, char)], i: usize) -> (usize, usize) {
    let ch = chars[i].1;
    if is_variation_selector(ch) || is_zero_width(ch) || is_emoji_modifier(ch) || is_keycap(ch) {
        return (i + 1, 0);
    }

    if is_regional_indicator(ch) && i + 1 < chars.len() && is_regional_indicator(chars[i + 1].1) {
        return (i + 2, 2);
    }

    let width = char_width(ch);
    if width == 2 {
        let mut j = i + 1;
        while j < chars.len()
            && (is_variation_selector(chars[j].1) || is_emoji_modifier(chars[j].1))
        {
            j += 1;
        }
        let mut saw_zwj = false;
        while j < chars.len() && chars[j].1 == '\u{200D}' {
            saw_zwj = true;
            j += 1;
            if j >= chars.len() {
                break;
            }
            j += 1;
            while j < chars.len()
                && (is_variation_selector(chars[j].1) || is_emoji_modifier(chars[j].1))
            {
                j += 1;
            }
        }
        if saw_zwj {
            return (j, 2);
        }
    }

    (i + 1, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn paddstr_truncate_and_pad() {
        let text = "Hello World";
        assert_eq!(pad_string(text, 5), "Hello");
        assert_eq!(pad_string(text, 12), "Hello World ");
    }

    #[test]
    fn wstr_trunc_respects_width() {
        let text = "\u{4E2D}\u{6587}\u{6D4B}\u{8BD5}"; // 中文测试
        let trunc = truncate_str_width(text, text.len(), 5);
        assert_eq!(trunc.width, 4);
        assert_eq!(text.get(..trunc.bytes).unwrap_or(""), "\u{4E2D}\u{6587}");
    }

    #[test]
    fn expand_tabs_basic() {
        let input = "ab\tcd";
        let expanded = expand_tabs(input, input.len(), 4).unwrap();
        assert_eq!(expanded, "ab  cd");
    }

    #[test]
    fn enter_fname_backspace_and_enter() {
        let mut out = Vec::new();
        let mut completion = FileCompletionData::default();
        let mut fname = "abc".to_string();
        let mut events = vec![
            Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ]
        .into_iter();

        let ret = mw_enter_fname_with(
            &mut out,
            "File",
            &mut fname,
            &mut completion,
            SelectFileFlags::NONE,
            || Ok(events.next().expect("event")),
        )
        .unwrap();
        assert_eq!(ret, 0);
        assert_eq!(fname, "abd");
    }

    #[test]
    fn enter_fname_escape_aborts() {
        let mut out = Vec::new();
        let mut completion = FileCompletionData::default();
        let mut fname = String::new();
        let mut events =
            vec![Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))].into_iter();

        let ret = mw_enter_fname_with(
            &mut out,
            "File",
            &mut fname,
            &mut completion,
            SelectFileFlags::NONE,
            || Ok(events.next().expect("event")),
        )
        .unwrap();
        assert_eq!(ret, -1);
    }

    #[test]
    fn scroll_state_new() {
        let scroll = ScrollState::new();
        assert_eq!(scroll.offset(), 0);
    }

    #[test]
    fn scroll_state_scroll_down() {
        let mut scroll = ScrollState::new();
        scroll.scroll_down(1, 10);
        assert_eq!(scroll.offset(), 1);
        scroll.scroll_down(5, 10);
        assert_eq!(scroll.offset(), 6);
        // Can't exceed max_offset
        scroll.scroll_down(100, 10);
        assert_eq!(scroll.offset(), 10);
    }

    #[test]
    fn scroll_state_scroll_up() {
        let mut scroll = ScrollState::new();
        scroll.scroll_down(5, 10);
        assert_eq!(scroll.offset(), 5);
        scroll.scroll_up(2);
        assert_eq!(scroll.offset(), 3);
        // Can't go below 0
        scroll.scroll_up(100);
        assert_eq!(scroll.offset(), 0);
    }

    #[test]
    fn scroll_state_scroll_to_bottom() {
        let mut scroll = ScrollState::new();
        scroll.scroll_to_bottom(5, 20);
        assert_eq!(scroll.offset(), 15);
        // When list fits in viewport
        scroll.scroll_to_bottom(20, 10);
        assert_eq!(scroll.offset(), 0);
    }

    #[test]
    fn scroll_state_scroll_to_top() {
        let mut scroll = ScrollState::new();
        scroll.scroll_down(5, 10);
        scroll.scroll_to_top();
        assert_eq!(scroll.offset(), 0);
    }

    #[test]
    fn scroll_state_max_offset() {
        assert_eq!(ScrollState::max_offset(20, 10), 10);
        assert_eq!(ScrollState::max_offset(10, 20), 0);
        assert_eq!(ScrollState::max_offset(5, 5), 0);
    }
}
