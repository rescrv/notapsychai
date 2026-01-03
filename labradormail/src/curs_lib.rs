//! Curses-style UI glue helpers.
//!
//! This module mirrors key behaviors from NeoMutt's curs_lib.c for width
//! handling, padding/truncation, and simple key prompts.

use std::io::Write;
use std::process::Command;

use crossterm::cursor::Hide;
use crossterm::event::read;
use crossterm::event::EnableMouseCapture;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use crossterm::style::Print;
use crossterm::terminal::enable_raw_mode;
use crossterm::terminal::Clear;
use crossterm::terminal::ClearType;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::ExecutableCommand;
use unicode_width::UnicodeWidthChar;

use crate::context::GuiContext;
use crate::root_window::redraw_global;
use crate::root_window::RootWindow;
use crate::terminal::Terminal;
use crate::window::MuttWindow;

/// Measures the display width of a character.
///
/// Matches wcwidth-style semantics and treats control/zero-width characters
/// as occupying zero cells.
pub fn mutt_char_width(ch: char) -> usize {
    if ch.is_control() {
        return 0;
    }

    if is_variation_selector(ch) || is_zero_width(ch) || is_emoji_modifier(ch) || is_keycap(ch) {
        return 0;
    }

    UnicodeWidthChar::width(ch).unwrap_or(1)
}

/// Measures a string's width in screen cells.
pub fn mutt_strwidth(s: &str) -> usize {
    mutt_strnwidth(s, s.len())
}

/// Measures a string's width in screen cells, up to `n` bytes.
pub fn mutt_strnwidth(s: &str, n: usize) -> usize {
    let mut width = 0usize;
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let (idx, ch) = chars[i];
        if idx >= n {
            break;
        }
        let bytes = ch.len_utf8();
        if idx + bytes > n {
            break;
        }
        let (next_i, cluster_width) = cluster_width_at(&chars, i);
        let end_byte = if next_i < chars.len() {
            chars[next_i].0
        } else {
            s.len()
        };
        if end_byte > n {
            break;
        }
        width += cluster_width;
        i = next_i;
    }
    width
}

/// Truncate a string by max byte length and max display width.
///
/// Returns the number of bytes that fit. Optionally writes the resulting
/// display width to `width`.
pub fn mutt_wstr_trunc(
    src: &str,
    maxlen: usize,
    maxwid: usize,
    width: Option<&mut usize>,
) -> usize {
    let mut used_width = 0usize;
    let mut used_len = 0usize;
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let (idx, ch) = chars[i];
        if idx >= maxlen {
            break;
        }
        let bytes = ch.len_utf8();
        if idx + bytes > maxlen {
            break;
        }
        if ch == '\n' {
            break;
        }
        let (next_i, cluster_width) = cluster_width_at(&chars, i);
        let end_byte = if next_i < chars.len() {
            chars[next_i].0
        } else {
            src.len()
        };
        if end_byte > maxlen {
            break;
        }
        if used_width + cluster_width > maxwid {
            break;
        }
        used_width += cluster_width;
        used_len = end_byte;
        i = next_i;
    }

    if let Some(out_width) = width {
        *out_width = used_width;
    }

    used_len
}

/// Pads or truncates a string to exactly `width` screen cells.
pub(crate) fn mutt_paddstr_string(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let mut used_width = 0usize;
    let used_bytes = mutt_wstr_trunc(s, s.len(), width, Some(&mut used_width));
    let mut out = s.get(..used_bytes).unwrap_or("").to_string();

    if used_width < width {
        out.extend(std::iter::repeat_n(' ', width - used_width));
    }

    out
}

/// Pads or truncates a string to the given width and draws it.
pub fn mutt_paddstr(
    win: &MuttWindow,
    out: &mut dyn Write,
    width: usize,
    s: &str,
) -> std::io::Result<()> {
    let padded = mutt_paddstr_string(s, width);
    win.addstr(out, &padded)
}

/// Writes a single character at the current cursor position.
pub fn mutt_addwch(win: &MuttWindow, out: &mut dyn Write, ch: char) -> std::io::Result<()> {
    win.addch(out, ch)
}

/// Prompt the user to press any key and wait for the key event.
fn mutt_any_key_to_continue_with<R>(
    out: &mut dyn Write,
    prompt: Option<&str>,
    mut read_event: R,
) -> std::io::Result<KeyEvent>
where
    R: FnMut() -> std::io::Result<Event>,
{
    let message = prompt.unwrap_or("Press any key to continue...");
    out.execute(Print(message))?;
    out.flush()?;

    loop {
        if let Event::Key(key) = read_event()? {
            return Ok(key);
        }
    }
}

/// Prompt the user to press any key and wait for the key event.
pub fn mutt_any_key_to_continue(
    out: &mut dyn Write,
    prompt: Option<&str>,
) -> std::io::Result<KeyEvent> {
    mutt_any_key_to_continue_with(out, prompt, read)
}

/// Prompt for a key and return it, or None if the user aborts (Ctrl+G).
fn mw_what_key_with<R>(out: &mut dyn Write, mut read_event: R) -> std::io::Result<Option<KeyEvent>>
where
    R: FnMut() -> std::io::Result<Event>,
{
    let abort = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
    out.execute(Print("Enter keys (Ctrl+G to abort): "))?;
    out.flush()?;

    loop {
        if let Event::Key(key) = read_event()? {
            if key == abort {
                return Ok(None);
            }
            return Ok(Some(key));
        }
    }
}

/// Prompt for a key and return it, or None if the user aborts (Ctrl+G).
pub fn mw_what_key(out: &mut dyn Write) -> std::io::Result<Option<KeyEvent>> {
    mw_what_key_with(out, read)
}

/// Beeps the terminal.
pub fn mutt_beep(out: &mut dyn Write) -> std::io::Result<()> {
    Terminal::beep(out)
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

/// Force a refresh of the screen.
pub fn mutt_refresh(out: &mut dyn Write) -> std::io::Result<()> {
    out.flush()
}

/// Force a hard refresh and reassert alternate screen usage.
pub fn mutt_need_hard_redraw(ctx: &mut GuiContext, out: &mut dyn Write) -> std::io::Result<()> {
    out.write_all(b"\x1b[?1049h")?;
    out.execute(Clear(ClearType::All))?;
    out.flush()?;
    let _ = redraw_global(ctx, out);
    Ok(())
}

/// Shutdown curses-style terminal state.
pub fn mutt_endwin(out: &mut dyn Write) -> std::io::Result<()> {
    RootWindow::cleanup(out)
}

fn build_editor_command(editor: &str, file: &str) -> Command {
    if editor.split_whitespace().count() > 1 {
        let escaped = file.replace('\'', "'\\''");
        let cmd = format!("{editor} '{escaped}'");
        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);
        command
    } else {
        let mut command = Command::new(editor);
        command.arg(file);
        command
    }
}

/// Let the user edit a file using the configured editor.
pub fn mutt_edit_file(out: &mut dyn Write, editor: &str, file: &str) -> std::io::Result<()> {
    mutt_endwin(out)?;

    let status = build_editor_command(editor, file)
        .status()
        .map_err(|e| std::io::Error::other(format!("failed to run editor '{editor}': {e}")))?;

    if !status.success() {
        return Err(std::io::Error::other("editor returned non-zero status"));
    }

    enable_raw_mode()
        .map_err(|e| std::io::Error::other(format!("failed to enable raw mode: {e}")))?;
    out.execute(EnterAlternateScreen)
        .map_err(|e| std::io::Error::other(format!("failed to enter alternate screen: {e}")))?;
    out.execute(EnableMouseCapture)
        .map_err(|e| std::io::Error::other(format!("failed to enable mouse capture: {e}")))?;
    out.execute(Hide)
        .map_err(|e| std::io::Error::other(format!("failed to hide cursor: {e}")))?;
    out.execute(Clear(ClearType::All))
        .map_err(|e| std::io::Error::other(format!("failed to clear screen: {e}")))?;
    Ok(())
}

fn mutt_query_exit_with<R>(out: &mut dyn Write, mut read_event: R) -> std::io::Result<bool>
where
    R: FnMut() -> std::io::Result<Event>,
{
    out.execute(Print("Exit NeoMutt without saving? (y/N) "))?;
    out.flush()?;

    loop {
        if let Event::Key(key) = read_event()? {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    out.execute(Print("\r\n"))?;
                    out.flush()?;
                    return Ok(true);
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => {
                    out.execute(Print("\r\n"))?;
                    out.flush()?;
                    return Ok(false);
                }
                KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    out.execute(Print("\r\n"))?;
                    out.flush()?;
                    return Ok(false);
                }
                _ => {}
            }
        }
    }
}

/// Ask the user if they want to exit.
pub fn mutt_query_exit(out: &mut dyn Write) -> std::io::Result<bool> {
    mutt_query_exit_with(out, read)
}

/// Convert tabs to spaces in a string.
pub fn mutt_str_expand_tabs(s: &str, len: usize, tabwidth: usize) -> Option<String> {
    if s.is_empty() || len == 0 || tabwidth < 1 {
        return None;
    }

    let mut out = String::new();
    let mut width = 0usize;

    for (idx, ch) in s.char_indices() {
        if idx >= len {
            break;
        }
        let bytes = ch.len_utf8();
        if idx + bytes > len {
            break;
        }

        if ch == '\t' {
            let indent = tabwidth - (width % tabwidth);
            out.extend(std::iter::repeat_n(' ', indent));
            width += indent;
        } else {
            out.push(ch);
            width += mutt_char_width(ch);
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

    let width = mutt_char_width(ch);
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

    #[test]
    fn strwidth_ascii() {
        assert_eq!(mutt_strwidth("hello"), 5);
    }

    #[test]
    fn strwidth_cjk() {
        let text = "\u{4E2D}\u{6587}"; // 中文
        assert_eq!(mutt_strwidth(text), 4);
    }

    #[test]
    fn strwidth_emoji() {
        let text = "\u{1F642}"; // 🙂
        assert_eq!(mutt_strwidth(text), 2);
    }

    #[test]
    fn strwidth_combining_mark() {
        let text = "\u{0065}\u{0301}"; // e + combining acute
        assert_eq!(mutt_strwidth(text), 1);
    }

    #[test]
    fn paddstr_truncate_and_pad() {
        let text = "Hello World";
        assert_eq!(mutt_paddstr_string(text, 5), "Hello");
        assert_eq!(mutt_paddstr_string(text, 12), "Hello World ");
    }

    #[test]
    fn paddstr_mixed_ascii_cjk() {
        let padded = mutt_paddstr_string("A\u{4E2D}", 4);
        assert_eq!(mutt_strwidth(&padded), 4);
    }

    #[test]
    fn wstr_trunc_respects_width() {
        let text = "\u{4E2D}\u{6587}\u{6D4B}\u{8BD5}"; // 中文测试
        let mut width = 0usize;
        let bytes = mutt_wstr_trunc(text, text.len(), 5, Some(&mut width));
        assert_eq!(width, 4);
        assert_eq!(text.get(..bytes).unwrap_or(""), "\u{4E2D}\u{6587}");
    }

    #[test]
    fn expand_tabs_basic() {
        let input = "ab\tcd";
        let expanded = mutt_str_expand_tabs(input, input.len(), 4).unwrap();
        assert_eq!(expanded, "ab  cd");
    }

    #[test]
    fn any_key_to_continue_returns_key() {
        let mut out = Vec::new();
        let mut events = vec![Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
        ))]
        .into_iter();
        let key = mutt_any_key_to_continue_with(&mut out, Some("Press a key"), || {
            Ok(events.next().expect("event"))
        })
        .unwrap();
        assert_eq!(key.code, KeyCode::Char('x'));
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Press a key"));
    }

    #[test]
    fn mw_what_key_ctrl_g_returns_none() {
        let mut out = Vec::new();
        let ctrl_g = Event::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
        let mut events = vec![ctrl_g].into_iter();
        let key = mw_what_key_with(&mut out, || Ok(events.next().expect("event"))).unwrap();
        assert!(key.is_none());
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Ctrl+G"));
    }

    #[test]
    fn query_exit_accepts_yes() {
        let mut out = Vec::new();
        let mut events = vec![Event::Key(KeyEvent::new(
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        ))]
        .into_iter();
        let exit = mutt_query_exit_with(&mut out, || Ok(events.next().expect("event"))).unwrap();
        assert!(exit);
    }

    #[test]
    fn query_exit_rejects_no_escape_and_ctrl_g() {
        let mut out = Vec::new();
        let mut events = vec![Event::Key(KeyEvent::new(
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        ))]
        .into_iter();
        let exit = mutt_query_exit_with(&mut out, || Ok(events.next().expect("event"))).unwrap();
        assert!(!exit);

        let mut out = Vec::new();
        let mut events =
            vec![Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))].into_iter();
        let exit = mutt_query_exit_with(&mut out, || Ok(events.next().expect("event"))).unwrap();
        assert!(!exit);

        let mut out = Vec::new();
        let mut events = vec![Event::Key(KeyEvent::new(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
        ))]
        .into_iter();
        let exit = mutt_query_exit_with(&mut out, || Ok(events.next().expect("event"))).unwrap();
        assert!(!exit);
    }

    #[test]
    fn build_editor_command_variants() {
        let command = build_editor_command("vi", "/tmp/file.txt");
        assert_eq!(command.get_program(), "vi");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, [std::ffi::OsStr::new("/tmp/file.txt")]);

        let command = build_editor_command("vim -u NONE", "/tmp/file.txt");
        assert_eq!(command.get_program(), "sh");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args[0], std::ffi::OsStr::new("-c"));
        assert!(args[1].to_string_lossy().contains("vim -u NONE"));
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
}
