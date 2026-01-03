//! Key bindings and help data generation.

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;

use crate::help_data::HelpData;
use crate::help_data::HelpItem;
use crate::opcodes::opcodes_get_description;
use crate::opcodes::OpCode;

/// Key descriptor for bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// Key code.
    pub code: KeyCode,
    /// Modifiers.
    pub modifiers: KeyModifiers,
}

impl Key {
    /// Creates a new key descriptor.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

/// Mapping from key to opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    /// Key to match.
    pub key: Key,
    /// Opcode produced.
    pub op: OpCode,
}

/// Lookup an opcode for a key event.
pub fn lookup_binding(bindings: &[KeyBinding], event: KeyEvent) -> Option<OpCode> {
    bindings.iter().find_map(|binding| {
        if key_matches(binding.key, event) {
            Some(binding.op)
        } else {
            None
        }
    })
}

fn key_matches(binding: Key, event: KeyEvent) -> bool {
    if binding.code == event.code && binding.modifiers == event.modifiers {
        return true;
    }

    if let (KeyCode::Char(bc), KeyCode::Char(ec)) = (binding.code, event.code) {
        if bc == ec && bc.is_uppercase() {
            let event_mods = event.modifiers;
            if binding.modifiers == KeyModifiers::NONE && event_mods == KeyModifiers::SHIFT {
                return true;
            }
            if binding.modifiers == KeyModifiers::SHIFT && event_mods == KeyModifiers::NONE {
                return true;
            }
        }
    }

    false
}

const fn key_char(ch: char) -> Key {
    Key::new(KeyCode::Char(ch), KeyModifiers::NONE)
}

const fn key_alt(ch: char) -> Key {
    Key::new(KeyCode::Char(ch), KeyModifiers::ALT)
}

const fn key_ctrl(ch: char) -> Key {
    Key::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
}

const fn key_special(code: KeyCode) -> Key {
    Key::new(code, KeyModifiers::NONE)
}

/// Default generic bindings (available everywhere).
pub fn generic_default_bindings() -> &'static [KeyBinding] {
    static BINDINGS: [KeyBinding; 46] = [
        KeyBinding {
            key: key_char('L'),
            op: OpCode::BottomPage,
        },
        KeyBinding {
            key: key_char(':'),
            op: OpCode::EnterCommand,
        },
        KeyBinding {
            key: key_special(KeyCode::Home),
            op: OpCode::FirstEntry,
        },
        KeyBinding {
            key: key_char('='),
            op: OpCode::FirstEntry,
        },
        KeyBinding {
            key: key_special(KeyCode::Enter),
            op: OpCode::GenericSelectEntry,
        },
        KeyBinding {
            key: key_special(KeyCode::Enter),
            op: OpCode::GenericSelectEntry,
        },
        KeyBinding {
            key: key_special(KeyCode::Enter),
            op: OpCode::GenericSelectEntry,
        },
        KeyBinding {
            key: key_char(']'),
            op: OpCode::HalfDown,
        },
        KeyBinding {
            key: key_char('['),
            op: OpCode::HalfUp,
        },
        KeyBinding {
            key: key_char('?'),
            op: OpCode::Help,
        },
        KeyBinding {
            key: key_char('1'),
            op: OpCode::Jump1,
        },
        KeyBinding {
            key: key_char('2'),
            op: OpCode::Jump2,
        },
        KeyBinding {
            key: key_char('3'),
            op: OpCode::Jump3,
        },
        KeyBinding {
            key: key_char('4'),
            op: OpCode::Jump4,
        },
        KeyBinding {
            key: key_char('5'),
            op: OpCode::Jump5,
        },
        KeyBinding {
            key: key_char('6'),
            op: OpCode::Jump6,
        },
        KeyBinding {
            key: key_char('7'),
            op: OpCode::Jump7,
        },
        KeyBinding {
            key: key_char('8'),
            op: OpCode::Jump8,
        },
        KeyBinding {
            key: key_char('9'),
            op: OpCode::Jump9,
        },
        KeyBinding {
            key: key_char('*'),
            op: OpCode::LastEntry,
        },
        KeyBinding {
            key: key_special(KeyCode::End),
            op: OpCode::LastEntry,
        },
        KeyBinding {
            key: key_char('M'),
            op: OpCode::MiddlePage,
        },
        KeyBinding {
            key: key_char('m'),
            op: OpCode::Mail,
        },
        KeyBinding {
            key: key_special(KeyCode::Down),
            op: OpCode::NextEntry,
        },
        KeyBinding {
            key: key_char('j'),
            op: OpCode::NextEntry,
        },
        KeyBinding {
            key: key_char('>'),
            op: OpCode::NextLine,
        },
        KeyBinding {
            key: key_special(KeyCode::PageDown),
            op: OpCode::NextPage,
        },
        KeyBinding {
            key: key_special(KeyCode::Right),
            op: OpCode::NextPage,
        },
        KeyBinding {
            key: key_char('z'),
            op: OpCode::NextPage,
        },
        KeyBinding {
            key: key_special(KeyCode::Up),
            op: OpCode::PrevEntry,
        },
        KeyBinding {
            key: key_char('k'),
            op: OpCode::PrevEntry,
        },
        KeyBinding {
            key: key_char('J'),
            op: OpCode::SidebarNext,
        },
        KeyBinding {
            key: key_char('K'),
            op: OpCode::SidebarPrev,
        },
        KeyBinding {
            key: key_char('<'),
            op: OpCode::PrevLine,
        },
        KeyBinding {
            key: key_special(KeyCode::Left),
            op: OpCode::PrevPage,
        },
        KeyBinding {
            key: key_special(KeyCode::PageUp),
            op: OpCode::PrevPage,
        },
        KeyBinding {
            key: key_char('Z'),
            op: OpCode::PrevPage,
        },
        KeyBinding {
            key: key_ctrl('l'),
            op: OpCode::Redraw,
        },
        KeyBinding {
            key: key_char('/'),
            op: OpCode::Search,
        },
        KeyBinding {
            key: key_char('n'),
            op: OpCode::SearchNext,
        },
        KeyBinding {
            key: key_alt('/'),
            op: OpCode::SearchReverse,
        },
        KeyBinding {
            key: key_char('!'),
            op: OpCode::ShellEscape,
        },
        KeyBinding {
            key: key_char('t'),
            op: OpCode::Tag,
        },
        KeyBinding {
            key: key_char(';'),
            op: OpCode::TagPrefix,
        },
        KeyBinding {
            key: key_char('H'),
            op: OpCode::TopPage,
        },
        KeyBinding {
            key: key_char('V'),
            op: OpCode::Version,
        },
    ];
    &BINDINGS
}

/// Default dialog bindings.
pub fn dialog_default_bindings() -> &'static [KeyBinding] {
    static BINDINGS: [KeyBinding; 1] = [KeyBinding {
        key: key_char('q'),
        op: OpCode::Quit,
    }];
    &BINDINGS
}

/// Builds help data from a list of key bindings.
pub fn help_data_from_bindings(bindings: &[KeyBinding]) -> HelpData {
    let mut items = Vec::new();
    for binding in bindings {
        items.push(HelpItem::new(
            key_to_string(binding.key),
            opcodes_get_description(binding.op as i32),
        ));
    }
    HelpData::from_items(items)
}

fn key_to_string(key: Key) -> String {
    let mut out = String::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        out.push_str("Ctrl-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        out.push_str("Alt-");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        out.push_str("Shift-");
    }
    if let KeyCode::Char(ch) = key.code {
        out.push(ch);
        return out;
    }
    match key.code {
        KeyCode::Esc => out.push_str("Esc"),
        KeyCode::Enter => out.push_str("Enter"),
        KeyCode::Tab => out.push_str("Tab"),
        KeyCode::Backspace => out.push_str("Backspace"),
        KeyCode::Left => out.push_str("Left"),
        KeyCode::Right => out.push_str("Right"),
        KeyCode::Up => out.push_str("Up"),
        KeyCode::Down => out.push_str("Down"),
        KeyCode::PageUp => out.push_str("PageUp"),
        KeyCode::PageDown => out.push_str("PageDown"),
        KeyCode::Home => out.push_str("Home"),
        KeyCode::End => out.push_str("End"),
        KeyCode::Insert => out.push_str("Insert"),
        KeyCode::Delete => out.push_str("Delete"),
        KeyCode::F(n) => out.push_str(&format!("F{}", n)),
        _ => out.push_str("Key"),
    };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matches_shift_normalizes_uppercase() {
        let binding = Key::new(KeyCode::Char('A'), KeyModifiers::NONE);
        let event = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert!(super::key_matches(binding, event));
    }

    #[test]
    fn key_matches_lowercase_shift_does_not_match() {
        let binding = Key::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SHIFT);
        assert!(!super::key_matches(binding, event));
    }

    #[test]
    fn lookup_binding_empty_list_returns_none() {
        let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(lookup_binding(&[], event).is_none());
    }

    #[test]
    fn lookup_binding_first_match_wins() {
        let bindings = [
            KeyBinding {
                key: Key::new(KeyCode::Char('q'), KeyModifiers::NONE),
                op: OpCode::Quit,
            },
            KeyBinding {
                key: Key::new(KeyCode::Char('q'), KeyModifiers::NONE),
                op: OpCode::Exit,
            },
        ];
        let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let op = lookup_binding(&bindings, event);
        assert_eq!(op, Some(OpCode::Quit));
    }

    #[test]
    fn key_matches_combined_modifiers() {
        let binding = Key::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        let event = KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert!(super::key_matches(binding, event));
    }

    #[test]
    fn key_to_string_variants() {
        let key = Key::new(KeyCode::Insert, KeyModifiers::NONE);
        assert_eq!(key_to_string(key), "Insert");
        let key = Key::new(KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(key_to_string(key), "Delete");
        let key = Key::new(KeyCode::F(2), KeyModifiers::NONE);
        assert_eq!(key_to_string(key), "F2");
    }

    #[test]
    fn help_data_from_bindings_includes_special_keys() {
        let bindings = [
            KeyBinding {
                key: Key::new(KeyCode::F(1), KeyModifiers::NONE),
                op: OpCode::Help,
            },
            KeyBinding {
                key: Key::new(KeyCode::F(12), KeyModifiers::NONE),
                op: OpCode::Help,
            },
            KeyBinding {
                key: Key::new(KeyCode::Insert, KeyModifiers::NONE),
                op: OpCode::Help,
            },
            KeyBinding {
                key: Key::new(KeyCode::Delete, KeyModifiers::NONE),
                op: OpCode::Help,
            },
            KeyBinding {
                key: Key::new(KeyCode::Home, KeyModifiers::NONE),
                op: OpCode::Help,
            },
        ];

        let help = help_data_from_bindings(&bindings);
        let keys: Vec<_> = help.items.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, ["F1", "F12", "Insert", "Delete", "Home"]);
    }
}
