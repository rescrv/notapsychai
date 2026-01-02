//! Help bar data structures.
//!
//! This module keeps the help data separate from window logic so it can be
//! shared across windows and compiled into a help bar string.

use std::fmt;

/// A single help item (key + description).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpItem {
    /// Key label shown in the help bar.
    pub key: String,
    /// Description shown in the help bar.
    pub description: String,
}

impl HelpItem {
    /// Creates a new help item.
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
        }
    }
}

/// A collection of help items.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HelpData {
    /// Ordered list of help items.
    pub items: Vec<HelpItem>,
}

impl HelpData {
    /// Creates empty help data.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates help data from a list of items.
    pub fn from_items(items: Vec<HelpItem>) -> Self {
        Self { items }
    }

    /// Returns true if there are no help items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Compiles the help data into a single-line string for the help bar.
    pub fn compile_line(&self) -> String {
        if self.items.is_empty() {
            return String::new();
        }

        let mut out = String::new();
        for (idx, item) in self.items.iter().enumerate() {
            if idx > 0 {
                out.push_str("  ");
            }
            out.push_str(&item.key);
            out.push(':');
            out.push(' ');
            out.push_str(&item.description);
        }
        out
    }
}

impl fmt::Display for HelpData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.compile_line())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_data_compile() {
        let data =
            HelpData::from_items(vec![HelpItem::new("q", "Quit"), HelpItem::new("?", "Help")]);
        assert_eq!(data.compile_line(), "q: Quit  ?: Help");
    }
}
