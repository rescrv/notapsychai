//! A recursive descent parser for Org-mode documents.
//!
//! The parser operates in two phases:
//! 1. Structure parsing: Parse headlines, blocks, and other line-oriented elements
//! 2. Object parsing: Parse inline markup, links, and timestamps within text
//!
//! This approach mirrors org-element.el from Emacs.

use crate::Bold;
use crate::CenterBlock;
use crate::Checkbox;
use crate::Clock;
use crate::Code;
use crate::Comment;
use crate::CommentBlock;
use crate::DateTime;
use crate::Document;
use crate::Drawer;
use crate::Duration;
use crate::Element;
use crate::ExampleBlock;
use crate::FixedWidth;
use crate::FootnoteReference;
use crate::Headline;
use crate::HorizontalRule;
use crate::Italic;
use crate::Keyword;
use crate::LineBreak;
use crate::Link;
use crate::LinkType;
use crate::ListItem;
use crate::ListType;
use crate::NodeProperty;
use crate::Object;
use crate::Paragraph;
use crate::PlainList;
use crate::Planning;
use crate::Priority;
use crate::PropertyDrawer;
use crate::QuoteBlock;
use crate::Repeater;
use crate::RepeaterType;
use crate::Section;
use crate::SourceBlock;
use crate::StatisticsCookie;
use crate::StrikeThrough;
use crate::Tag;
use crate::Text;
use crate::TimeUnit;
use crate::Timestamp;
use crate::TimestampType;
use crate::TodoKeyword;
use crate::Underline;
use crate::ValidationError;
use crate::Verbatim;
use crate::VerseBlock;
use crate::WarningDelay;
use crate::WarningType;
use crate::validate_todo_keyword;

///////////////////////////////////////////// ParseError /////////////////////////////////////////////

/// An error that occurred during parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    /// The line number where the error occurred (1-indexed).
    pub line: usize,
    /// The column number where the error occurred (1-indexed).
    pub column: usize,
    /// A description of the error.
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

///////////////////////////////////////////// ParseConfig ////////////////////////////////////////////

/// Configuration for the parser.
///
/// TODO and DONE keywords must contain only uppercase ASCII letters.
#[derive(Clone, Debug)]
pub struct ParseConfig {
    /// TODO keywords that represent incomplete states.
    pub todo_keywords: Vec<String>,
    /// TODO keywords that represent complete states.
    pub done_keywords: Vec<String>,
}

impl ParseConfig {
    /// Create a new parse configuration with validation.
    ///
    /// # Errors
    ///
    /// Returns an error if any keyword contains non-uppercase ASCII letters.
    pub fn new(
        todo_keywords: Vec<String>,
        done_keywords: Vec<String>,
    ) -> Result<Self, ValidationError> {
        for kw in &todo_keywords {
            validate_todo_keyword(kw)?;
        }
        for kw in &done_keywords {
            validate_todo_keyword(kw)?;
        }
        Ok(Self {
            todo_keywords,
            done_keywords,
        })
    }
}

impl Default for ParseConfig {
    fn default() -> Self {
        Self {
            todo_keywords: vec!["TODO".to_string()],
            done_keywords: vec!["DONE".to_string()],
        }
    }
}

/////////////////////////////////////////////// Parser ///////////////////////////////////////////////

/// A recursive descent parser for Org-mode documents.
pub struct Parser<'a> {
    /// The input text.
    input: &'a str,
    /// Current position in the input (byte offset).
    pos: usize,
    /// Current line number (1-indexed).
    line: usize,
    /// Current column number (1-indexed).
    column: usize,
    /// Parser configuration.
    config: ParseConfig,
}

impl<'a> Parser<'a> {
    /// Create a new parser with the given input and default configuration.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
            column: 1,
            config: ParseConfig::default(),
        }
    }

    /// Create a new parser with custom configuration.
    pub fn with_config(input: &'a str, config: ParseConfig) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
            column: 1,
            config,
        }
    }

    /// Parse the entire document.
    pub fn parse(&mut self) -> Result<Document, ParseError> {
        let mut doc = Document::default();

        // Parse zeroth section (content before first headline)
        let zeroth_elements = self.parse_section_elements(0)?;
        if !zeroth_elements.is_empty() {
            doc.zeroth_section = Some(Section {
                elements: zeroth_elements,
            });
        }

        // Parse top-level headlines
        while !self.is_eof() {
            if self.looking_at_headline() {
                let headline = self.parse_headline()?;
                doc.headlines.push(headline);
            } else {
                // Should not happen if parse_section_elements worked correctly
                break;
            }
        }

        Ok(doc)
    }

    ///////////////////////////////////// Position Management ////////////////////////////////////////

    /// Check if we've reached the end of input.
    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Get the remaining input.
    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    /// Peek at the current character.
    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// Advance by one character.
    fn advance_char(&mut self) {
        if let Some(c) = self.peek_char() {
            self.pos += c.len_utf8();
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
    }

    /// Advance by n bytes (caller must ensure this is at a char boundary).
    fn advance_bytes(&mut self, n: usize) {
        let text = &self.input[self.pos..self.pos + n];
        for c in text.chars() {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.pos += n;
    }

    /// Get the current line (without newline).
    fn current_line(&self) -> &'a str {
        let start = self.pos;
        let end = self
            .remaining()
            .find('\n')
            .map_or(self.input.len(), |i| self.pos + i);
        &self.input[start..end]
    }

    /// Consume the current line and return it (including advancing past newline).
    fn consume_line(&mut self) -> &'a str {
        let line = self.current_line();
        self.advance_bytes(line.len());
        if self.peek_char() == Some('\n') {
            self.advance_char();
        }
        line
    }

    /// Create an error at the current position.
    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            line: self.line,
            column: self.column,
            message: message.into(),
        }
    }

    ////////////////////////////////////// Headline Parsing //////////////////////////////////////////

    /// Check if the current position is at a headline.
    fn looking_at_headline(&self) -> bool {
        let line = self.current_line();
        if !line.starts_with('*') {
            return false;
        }
        // Must have only stars followed by space or end of line
        let stars = line.chars().take_while(|&c| c == '*').count();
        let after_stars = &line[stars..];
        after_stars.is_empty() || after_stars.starts_with(' ')
    }

    /// Get the headline level at the current position, or 0 if not a headline.
    fn headline_level(&self) -> u8 {
        if !self.looking_at_headline() {
            return 0;
        }
        let line = self.current_line();
        line.chars().take_while(|&c| c == '*').count() as u8
    }

    /// Parse a headline and its contents.
    fn parse_headline(&mut self) -> Result<Headline, ParseError> {
        let line = self.consume_line();
        let mut headline = Headline::default();

        // Parse level (stars)
        let stars: String = line.chars().take_while(|&c| c == '*').collect();
        headline.level = stars.len() as u8;

        // Parse rest of headline
        let rest = line[stars.len()..].trim_start();
        let (keyword, priority, commented, title_str, tags) = self.parse_headline_components(rest);

        headline.keyword = keyword;
        headline.priority = priority;
        headline.commented = commented;
        headline.title = self.parse_objects(title_str);
        headline.tags = tags;

        // Parse planning line if present
        if self.looking_at_planning() {
            headline.planning = Some(self.parse_planning()?);
        }

        // Parse property drawer if present
        if self.looking_at_property_drawer() {
            headline.properties = Some(self.parse_property_drawer()?);
        }

        // Parse section content
        let section_elements = self.parse_section_elements(headline.level)?;
        if !section_elements.is_empty() {
            headline.section = Some(Section {
                elements: section_elements,
            });
        }

        // Parse child headlines
        while !self.is_eof() {
            let child_level = self.headline_level();
            if child_level > headline.level {
                let child = self.parse_headline()?;
                headline.children.push(child);
            } else {
                break;
            }
        }

        Ok(headline)
    }

    /// Parse headline components: KEYWORD, PRIORITY, COMMENT, TITLE, TAGS.
    fn parse_headline_components<'b>(
        &self,
        text: &'b str,
    ) -> (
        Option<TodoKeyword>,
        Option<Priority>,
        bool,
        &'b str,
        Vec<Tag>,
    ) {
        let mut rest = text;
        let mut keyword = None;
        let mut priority = None;
        let mut commented = false;

        // Check for TODO keyword (config is pre-validated, so unwrap is safe)
        if let Some(first_word) = rest.split_whitespace().next() {
            if self.config.todo_keywords.contains(&first_word.to_string()) {
                keyword = Some(
                    TodoKeyword::new(first_word, false).expect("config keywords are pre-validated"),
                );
                rest = rest[first_word.len()..].trim_start();
            } else if self.config.done_keywords.contains(&first_word.to_string()) {
                keyword = Some(
                    TodoKeyword::new(first_word, true).expect("config keywords are pre-validated"),
                );
                rest = rest[first_word.len()..].trim_start();
            }
        }

        // Check for priority [#A] (we verify it's uppercase before constructing)
        if rest.starts_with("[#") && rest.len() >= 4 && rest.chars().nth(3) == Some(']') {
            let priority_char = rest.chars().nth(2).unwrap();
            if priority_char.is_ascii_uppercase() {
                priority = Some(
                    Priority::new(priority_char).expect("verified uppercase before constructing"),
                );
                rest = rest[4..].trim_start();
            }
        }

        // Check for COMMENT keyword
        if rest
            .split_whitespace()
            .next()
            .is_some_and(|w| w == "COMMENT")
        {
            commented = true;
            rest = rest[7..].trim_start();
        }

        // Parse tags from end
        let (title, tags) = self.extract_tags(rest);

        (keyword, priority, commented, title, tags)
    }

    /// Extract tags from the end of a headline title.
    fn extract_tags<'b>(&self, text: &'b str) -> (&'b str, Vec<Tag>) {
        let trimmed = text.trim_end();

        // Tags must be at the end, preceded by whitespace, format :tag1:tag2:
        if let Some(tag_start) = trimmed.rfind(|c: char| c.is_whitespace()) {
            let potential_tags = &trimmed[tag_start + 1..];
            if potential_tags.starts_with(':')
                && potential_tags.ends_with(':')
                && potential_tags.len() > 2
            {
                let tag_str = &potential_tags[1..potential_tags.len() - 1];
                // Validate each tag individually (filter out invalid ones)
                let tags: Vec<Tag> = tag_str
                    .split(':')
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| Tag::new(s).ok())
                    .collect();
                if !tags.is_empty() {
                    return (trimmed[..tag_start].trim_end(), tags);
                }
            }
        }

        (trimmed, vec![])
    }

    ////////////////////////////////////// Planning Parsing //////////////////////////////////////////

    /// Check if the current line is a planning line.
    fn looking_at_planning(&self) -> bool {
        let line = self.current_line().trim_start();
        line.starts_with("DEADLINE:")
            || line.starts_with("SCHEDULED:")
            || line.starts_with("CLOSED:")
    }

    /// Parse a planning line.
    fn parse_planning(&mut self) -> Result<Planning, ParseError> {
        let line = self.consume_line();
        let mut planning = Planning::default();

        let mut rest = line.trim();

        // Parse each planning keyword
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix("DEADLINE:") {
                rest = after.trim_start();
                if let Some((ts, remaining)) = self.parse_timestamp_from_str(rest) {
                    planning.deadline = Some(ts);
                    rest = remaining.trim_start();
                }
            } else if let Some(after) = rest.strip_prefix("SCHEDULED:") {
                rest = after.trim_start();
                if let Some((ts, remaining)) = self.parse_timestamp_from_str(rest) {
                    planning.scheduled = Some(ts);
                    rest = remaining.trim_start();
                }
            } else if let Some(after) = rest.strip_prefix("CLOSED:") {
                rest = after.trim_start();
                if let Some((ts, remaining)) = self.parse_timestamp_from_str(rest) {
                    planning.closed = Some(ts);
                    rest = remaining.trim_start();
                }
            } else {
                break;
            }
        }

        Ok(planning)
    }

    /////////////////////////////////// Property Drawer Parsing //////////////////////////////////////

    /// Check if the current line starts a property drawer.
    fn looking_at_property_drawer(&self) -> bool {
        let line = self.current_line().trim();
        line.eq_ignore_ascii_case(":PROPERTIES:")
    }

    /// Parse a property drawer.
    fn parse_property_drawer(&mut self) -> Result<PropertyDrawer, ParseError> {
        // Consume :PROPERTIES: line
        self.consume_line();

        let mut properties = Vec::new();

        while !self.is_eof() {
            let line = self.current_line().trim();

            if line.eq_ignore_ascii_case(":END:") {
                self.consume_line();
                break;
            }

            // Parse property :NAME: VALUE
            if let Some(stripped) = line.strip_prefix(':')
                && let Some(colon_end) = stripped.find(':')
            {
                let name = &stripped[..colon_end];
                let value = stripped[colon_end + 1..].trim();
                // Only add valid properties, skip invalid ones
                if let Ok(prop) = NodeProperty::new(name, value) {
                    properties.push(prop);
                }
            }

            self.consume_line();
        }

        Ok(PropertyDrawer { properties })
    }

    ///////////////////////////////////// Section/Element Parsing ////////////////////////////////////

    /// Parse section elements until we hit a headline of equal or lower level.
    fn parse_section_elements(&mut self, current_level: u8) -> Result<Vec<Element>, ParseError> {
        let mut elements = Vec::new();

        while !self.is_eof() {
            // Stop if we hit a headline
            let hl_level = self.headline_level();
            if hl_level > 0 && hl_level <= current_level {
                break;
            }
            if hl_level > 0 {
                // Child headline, handled elsewhere
                break;
            }

            // Try to parse various elements
            if let Some(element) = self.try_parse_element()? {
                elements.push(element);
            } else {
                // Skip blank lines
                let line = self.current_line();
                if line.trim().is_empty() {
                    self.consume_line();
                } else {
                    // Shouldn't reach here
                    break;
                }
            }
        }

        Ok(elements)
    }

    /// Try to parse an element, returning None if the line is blank or unrecognized.
    fn try_parse_element(&mut self) -> Result<Option<Element>, ParseError> {
        let line = self.current_line();

        if line.trim().is_empty() {
            return Ok(None);
        }

        // Check for various element types
        if self.looking_at_headline() {
            return Ok(None);
        }

        // Horizontal rule: 5+ hyphens
        if line.trim().chars().all(|c| c == '-') && line.trim().len() >= 5 {
            self.consume_line();
            return Ok(Some(Element::HorizontalRule(HorizontalRule)));
        }

        // Comment line: # followed by space or newline
        if line.trim().starts_with("# ") || line.trim() == "#" {
            let comment_text = line.trim().strip_prefix("# ").unwrap_or("").to_string();
            self.consume_line();
            return Ok(Some(Element::Comment(Comment {
                value: comment_text,
            })));
        }

        // Fixed width: starts with ": "
        if line.starts_with(": ") || line == ":" {
            return Ok(Some(self.parse_fixed_width()?));
        }

        // Keyword: #+KEY: VALUE
        if line.trim_start().starts_with("#+") {
            return self.parse_keyword_or_block();
        }

        // Drawer: :NAME:
        if self.looking_at_drawer() {
            return Ok(Some(Element::Drawer(self.parse_drawer()?)));
        }

        // Plain list: starts with bullet
        if self.looking_at_list_item() {
            return Ok(Some(Element::PlainList(self.parse_plain_list()?)));
        }

        // Clock: CLOCK:
        if line.trim_start().starts_with("CLOCK:") {
            return Ok(Some(Element::Clock(self.parse_clock()?)));
        }

        // Default: paragraph
        Ok(Some(Element::Paragraph(self.parse_paragraph()?)))
    }

    ////////////////////////////////////// Paragraph Parsing /////////////////////////////////////////

    /// Parse a paragraph (consecutive non-blank, non-element lines).
    fn parse_paragraph(&mut self) -> Result<Paragraph, ParseError> {
        let mut text = String::new();

        while !self.is_eof() {
            let line = self.current_line();

            // Stop conditions
            if line.trim().is_empty() {
                break;
            }
            if self.looking_at_headline() {
                break;
            }
            if line.trim_start().starts_with("#+") {
                break;
            }
            if self.looking_at_drawer() {
                break;
            }
            if self.looking_at_list_item() {
                break;
            }
            if line.starts_with(": ") || line == ":" {
                break;
            }
            if line.trim().starts_with("# ") || line.trim() == "#" {
                break;
            }
            if line.trim().chars().all(|c| c == '-') && line.trim().len() >= 5 {
                break;
            }
            if line.trim_start().starts_with("CLOCK:") {
                break;
            }

            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(self.consume_line());
        }

        Ok(Paragraph {
            contents: self.parse_objects(&text),
        })
    }

    ///////////////////////////////////// Fixed Width Parsing ////////////////////////////////////////

    /// Parse fixed-width content.
    fn parse_fixed_width(&mut self) -> Result<Element, ParseError> {
        let mut lines: Vec<&str> = Vec::new();

        while !self.is_eof() {
            let line = self.current_line();
            if let Some(stripped) = line.strip_prefix(": ") {
                lines.push(stripped);
                self.consume_line();
            } else if line == ":" {
                lines.push("");
                self.consume_line();
            } else {
                break;
            }
        }

        // Trim trailing empty lines
        while lines.last() == Some(&"") {
            lines.pop();
        }

        Ok(Element::FixedWidth(FixedWidth {
            value: lines.join("\n"),
        }))
    }

    ///////////////////////////////////// Keyword/Block Parsing //////////////////////////////////////

    /// Parse a keyword or block.
    fn parse_keyword_or_block(&mut self) -> Result<Option<Element>, ParseError> {
        let line = self.current_line();
        let trimmed = line.trim_start();

        // Check for block begin
        if trimmed.to_uppercase().starts_with("#+BEGIN_") {
            return self.parse_block();
        }

        // Regular keyword
        if let Some(rest) = trimmed.strip_prefix("#+")
            && let Some(colon_pos) = rest.find(':')
        {
            let key = rest[..colon_pos].to_string();
            let value = rest[colon_pos + 1..].trim().to_string();
            self.consume_line();
            return Ok(Some(Element::Keyword(Keyword { key, value })));
        }

        self.consume_line();
        Ok(None)
    }

    /// Parse a block (#+BEGIN_X ... #+END_X).
    fn parse_block(&mut self) -> Result<Option<Element>, ParseError> {
        let line = self.consume_line();
        let trimmed = line.trim_start();

        // Extract block type and arguments
        let begin_rest = trimmed
            .strip_prefix("#+BEGIN_")
            .or_else(|| trimmed.strip_prefix("#+begin_"))
            .unwrap_or("");

        let (block_type, args) = if let Some(space_pos) = begin_rest.find(' ') {
            (
                begin_rest[..space_pos].to_uppercase(),
                Some(begin_rest[space_pos + 1..].to_string()),
            )
        } else {
            (begin_rest.trim().to_uppercase(), None)
        };

        let end_marker = format!("#+END_{}", block_type);
        let end_marker_lower = end_marker.to_lowercase();

        // Collect block contents
        let mut contents = String::new();
        while !self.is_eof() {
            let current = self.current_line();
            let current_trimmed = current.trim();
            if current_trimmed.eq_ignore_ascii_case(&end_marker)
                || current_trimmed
                    .to_lowercase()
                    .starts_with(&end_marker_lower)
            {
                self.consume_line();
                break;
            }
            if !contents.is_empty() {
                contents.push('\n');
            }
            contents.push_str(self.consume_line());
        }

        let element = match block_type.as_str() {
            "SRC" => {
                let (language, arguments) = if let Some(ref a) = args {
                    let mut parts = a.splitn(2, ' ');
                    (
                        parts.next().map(|s| s.to_string()),
                        parts.next().map(|s| s.to_string()),
                    )
                } else {
                    (None, None)
                };
                Element::SourceBlock(SourceBlock {
                    language,
                    arguments,
                    contents,
                })
            }
            "EXAMPLE" => Element::ExampleBlock(ExampleBlock { contents }),
            "QUOTE" => {
                let mut inner_parser = Parser::new(&contents);
                let elements = inner_parser.parse_section_elements(0)?;
                Element::QuoteBlock(QuoteBlock { contents: elements })
            }
            "VERSE" => Element::VerseBlock(VerseBlock {
                contents: self.parse_objects(&contents),
            }),
            "CENTER" => {
                let mut inner_parser = Parser::new(&contents);
                let elements = inner_parser.parse_section_elements(0)?;
                Element::CenterBlock(CenterBlock { contents: elements })
            }
            "COMMENT" => Element::CommentBlock(CommentBlock { contents }),
            _ => {
                // Unknown block type, treat as generic drawer-like structure
                return Ok(None);
            }
        };

        Ok(Some(element))
    }

    /////////////////////////////////////// Drawer Parsing ///////////////////////////////////////////

    /// Check if current line starts a drawer.
    fn looking_at_drawer(&self) -> bool {
        let line = self.current_line().trim();
        if line.starts_with(':') && line.ends_with(':') && line.len() > 2 {
            let name = &line[1..line.len() - 1];
            // Not a property drawer (handled separately) and valid drawer name
            !name.eq_ignore_ascii_case("PROPERTIES")
                && !name.eq_ignore_ascii_case("END")
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        } else {
            false
        }
    }

    /// Parse a drawer.
    fn parse_drawer(&mut self) -> Result<Drawer, ParseError> {
        let line = self.consume_line();
        let trimmed = line.trim();
        let name = trimmed[1..trimmed.len() - 1].to_string();

        let mut contents_text = String::new();
        while !self.is_eof() {
            let current = self.current_line().trim();
            if current.eq_ignore_ascii_case(":END:") {
                self.consume_line();
                break;
            }
            if !contents_text.is_empty() {
                contents_text.push('\n');
            }
            contents_text.push_str(self.consume_line());
        }

        let mut inner_parser = Parser::new(&contents_text);
        let contents = inner_parser.parse_section_elements(0)?;

        // Name was already validated by looking_at_drawer
        Drawer::new(name, contents).map_err(|e| self.error(e.message))
    }

    ////////////////////////////////////// Plain List Parsing ////////////////////////////////////////

    /// Check if current line is a list item.
    fn looking_at_list_item(&self) -> bool {
        let line = self.current_line();
        let trimmed = line.trim_start();

        // Unordered: -, +, or * followed by space
        if trimmed.starts_with("- ")
            || trimmed.starts_with("+ ")
            || (trimmed.starts_with("* ") && !self.looking_at_headline())
        {
            return true;
        }

        // Ordered: number/letter followed by . or ) and space
        let mut chars = trimmed.chars().peekable();
        let first = chars.next();
        match first {
            Some(c) if c.is_ascii_digit() => {
                // Consume remaining digits
                while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                    chars.next();
                }
                matches!(
                    (chars.next(), chars.next()),
                    (Some('.'), Some(' ')) | (Some(')'), Some(' '))
                )
            }
            Some(c) if c.is_ascii_alphabetic() => {
                matches!(
                    (chars.next(), chars.next()),
                    (Some('.'), Some(' ')) | (Some(')'), Some(' '))
                )
            }
            _ => false,
        }
    }

    /// Get the indentation level of the current line.
    fn current_indentation(&self) -> usize {
        self.current_line()
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .map(|c| if c == '\t' { 8 } else { 1 })
            .sum()
    }

    /// Parse a plain list.
    fn parse_plain_list(&mut self) -> Result<PlainList, ParseError> {
        let mut items = Vec::new();
        let list_indent = self.current_indentation();
        let mut list_type = None;

        while !self.is_eof() && self.looking_at_list_item() {
            let item_indent = self.current_indentation();
            if item_indent < list_indent {
                break;
            }
            if item_indent > list_indent && !items.is_empty() {
                // This is a sub-list item, will be handled recursively
                break;
            }

            let (item, item_type) = self.parse_list_item()?;
            if list_type.is_none() {
                list_type = Some(item_type);
            }
            items.push(item);

            // Skip blank lines between items
            while !self.is_eof() && self.current_line().trim().is_empty() {
                self.consume_line();
            }
        }

        Ok(PlainList {
            list_type: list_type.unwrap_or(ListType::Unordered),
            items,
        })
    }

    /// Parse a single list item.
    fn parse_list_item(&mut self) -> Result<(ListItem, ListType), ParseError> {
        let line = self.consume_line();
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        // Determine bullet and list type
        let (bullet, list_type, rest) = self.parse_list_bullet(trimmed);

        // Check for counter set [@N]
        let (counter_set, rest) = if rest.starts_with("[@") {
            if let Some(end) = rest.find(']') {
                let num_str = &rest[2..end];
                let counter = num_str.parse().ok();
                (counter, rest[end + 1..].trim_start())
            } else {
                (None, rest)
            }
        } else {
            (None, rest)
        };

        // Check for checkbox
        let (checkbox, rest) = if let Some(stripped) = rest.strip_prefix("[ ] ") {
            (Some(Checkbox::Unchecked), stripped)
        } else if let Some(stripped) = rest.strip_prefix("[X] ") {
            (Some(Checkbox::Checked), stripped)
        } else if let Some(stripped) = rest.strip_prefix("[x] ") {
            (Some(Checkbox::Checked), stripped)
        } else if let Some(stripped) = rest.strip_prefix("[-] ") {
            (Some(Checkbox::Partial), stripped)
        } else {
            (None, rest)
        };

        // Check for description list tag
        let (tag, contents_start) =
            if list_type == ListType::Unordered || list_type == ListType::Description {
                if let Some(sep) = rest.find(" :: ") {
                    let tag_text = &rest[..sep];
                    (
                        Some(self.parse_objects(tag_text)),
                        rest[sep + 4..].to_string(),
                    )
                } else {
                    (None, rest.to_string())
                }
            } else {
                (None, rest.to_string())
            };

        let actual_list_type = if tag.is_some() {
            ListType::Description
        } else {
            list_type
        };

        // Collect continuation lines and nested content
        let mut content_text = contents_start;
        let content_indent = indent + bullet.len() + 1;

        while !self.is_eof() {
            let next_line = self.current_line();
            if next_line.trim().is_empty() {
                // Blank line might end the item or separate paragraphs
                break;
            }

            let next_indent = self.current_indentation();
            if next_indent >= content_indent {
                content_text.push('\n');
                let line = self.consume_line();
                // Strip the content indent so inner parser sees text at base indent
                if line.len() >= content_indent {
                    content_text.push_str(&line[content_indent..]);
                } else {
                    content_text.push_str(line.trim_start());
                }
            } else if self.looking_at_list_item() && next_indent == indent {
                // Same-level list item
                break;
            } else {
                break;
            }
        }

        // Parse content as elements
        let mut inner_parser = Parser::new(&content_text);
        let contents = inner_parser.parse_section_elements(0)?;

        Ok((
            ListItem {
                bullet,
                counter_set,
                checkbox,
                tag,
                contents,
            },
            actual_list_type,
        ))
    }

    /// Parse the bullet/counter from a list item.
    fn parse_list_bullet<'b>(&self, text: &'b str) -> (String, ListType, &'b str) {
        // Unordered bullets
        if let Some(rest) = text.strip_prefix("- ") {
            return ("-".to_string(), ListType::Unordered, rest);
        }
        if let Some(rest) = text.strip_prefix("+ ") {
            return ("+".to_string(), ListType::Unordered, rest);
        }
        if let Some(rest) = text.strip_prefix("* ") {
            return ("*".to_string(), ListType::Unordered, rest);
        }

        // Ordered bullets
        let mut chars = text.chars().peekable();
        let mut bullet = String::new();

        // Collect digits or single letter
        if let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                    bullet.push(chars.next().unwrap());
                }
            } else if c.is_ascii_alphabetic() {
                bullet.push(chars.next().unwrap());
            }
        }

        // Collect . or )
        if chars.peek().is_some_and(|&c| c == '.' || c == ')') {
            bullet.push(chars.next().unwrap());
        }

        let rest_start = bullet.len();
        if text.len() > rest_start && text.chars().nth(rest_start) == Some(' ') {
            return (bullet, ListType::Ordered, &text[rest_start + 1..]);
        }

        // Fallback
        ("-".to_string(), ListType::Unordered, text)
    }

    //////////////////////////////////////// Clock Parsing ///////////////////////////////////////////

    /// Parse a CLOCK entry.
    fn parse_clock(&mut self) -> Result<Clock, ParseError> {
        let line = self.consume_line();
        let rest = line
            .trim_start()
            .strip_prefix("CLOCK:")
            .unwrap_or("")
            .trim();

        // Parse timestamp (possibly a range)
        let (timestamp, remaining) = self
            .parse_timestamp_from_str(rest)
            .ok_or_else(|| self.error("expected timestamp after CLOCK:"))?;

        // Check for duration
        let duration = if let Some(dur_start) = remaining.find("=>") {
            let dur_str = remaining[dur_start + 2..].trim();
            self.parse_duration(dur_str)
        } else {
            None
        };

        Ok(Clock {
            timestamp,
            duration,
        })
    }

    /// Parse a duration string like "1:30".
    fn parse_duration(&self, text: &str) -> Option<Duration> {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() == 2 {
            let hours = parts[0].parse().ok()?;
            let minutes = parts[1].parse().ok()?;
            Some(Duration { hours, minutes })
        } else {
            None
        }
    }

    /////////////////////////////////////// Object Parsing ///////////////////////////////////////////

    /// Parse objects (inline elements) from text.
    fn parse_objects(&self, text: &str) -> Vec<Object> {
        let mut objects = Vec::new();
        let mut pos = 0;

        while pos < text.len() {
            // Look for special characters that might start an object
            let remaining = &text[pos..];

            // Try to parse various object types
            if let Some((obj, len)) = self.try_parse_object(remaining) {
                objects.push(obj);
                pos += len;
                continue;
            }

            // Accumulate plain text until next special character
            let text_end = self.find_next_object_start(remaining);
            if text_end > 0 {
                objects.push(Object::Text(Text {
                    value: remaining[..text_end].to_string(),
                }));
                pos += text_end;
            } else {
                // Single character that didn't start an object
                let c = remaining.chars().next().unwrap();
                if let Some(Object::Text(t)) = objects.last_mut() {
                    t.value.push(c);
                } else {
                    objects.push(Object::Text(Text {
                        value: c.to_string(),
                    }));
                }
                pos += c.len_utf8();
            }
        }

        // Consolidate adjacent text objects
        self.consolidate_text_objects(objects)
    }

    /// Find the position of the next potential object start.
    fn find_next_object_start(&self, text: &str) -> usize {
        let special = ['*', '/', '_', '+', '=', '~', '[', '<', '\\'];
        text.find(|c| special.contains(&c)).unwrap_or(text.len())
    }

    /// Try to parse an object at the current position.
    fn try_parse_object(&self, text: &str) -> Option<(Object, usize)> {
        // Line break: \\ at end of line (consumes the newline too)
        if text.starts_with("\\\\\n") {
            return Some((Object::LineBreak(LineBreak), 3));
        }
        if text.ends_with("\\\\") && text.len() == 2 {
            return Some((Object::LineBreak(LineBreak), 2));
        }

        // Timestamp
        if (text.starts_with('<') || text.starts_with('['))
            && let Some((ts, len)) = self.try_parse_timestamp(text)
        {
            return Some((Object::Timestamp(ts), len));
        }

        // Link: [[...]] or <...>
        if text.starts_with("[[")
            && let Some((link, len)) = self.try_parse_link(text)
        {
            return Some((Object::Link(link), len));
        }

        // Footnote reference
        if text.starts_with("[fn:")
            && let Some((fnref, len)) = self.try_parse_footnote_ref(text)
        {
            return Some((Object::FootnoteReference(fnref), len));
        }

        // Statistics cookie
        if text.starts_with('[')
            && let Some((cookie, len)) = self.try_parse_statistics_cookie(text)
        {
            return Some((Object::StatisticsCookie(cookie), len));
        }

        // Text markup
        if let Some((obj, len)) = self.try_parse_markup(text) {
            return Some((obj, len));
        }

        None
    }

    /// Try to parse text markup (*bold*, /italic/, etc.).
    fn try_parse_markup(&self, text: &str) -> Option<(Object, usize)> {
        let first_char = text.chars().next()?;
        let marker = match first_char {
            '*' | '/' | '_' | '+' | '=' | '~' => first_char,
            _ => return None,
        };

        // Check pre-condition: must be at start or after whitespace/punctuation
        // (simplified check - we'd need more context for full compliance)

        // Find the closing marker
        let rest = &text[1..];
        let close_pos = rest.find(marker)?;

        // Verify there's no whitespace after opening or before closing
        if close_pos == 0 {
            return None;
        }
        let inner = &rest[..close_pos];
        if inner.starts_with(|c: char| c.is_whitespace())
            || inner.ends_with(|c: char| c.is_whitespace())
        {
            return None;
        }

        let len = close_pos + 2; // marker + content + marker

        let obj = match marker {
            '*' => Object::Bold(Bold {
                contents: self.parse_objects(inner),
            }),
            '/' => Object::Italic(Italic {
                contents: self.parse_objects(inner),
            }),
            '_' => Object::Underline(Underline {
                contents: self.parse_objects(inner),
            }),
            '+' => Object::StrikeThrough(StrikeThrough {
                contents: self.parse_objects(inner),
            }),
            '=' => Object::Verbatim(Verbatim {
                value: inner.to_string(),
            }),
            '~' => Object::Code(Code {
                value: inner.to_string(),
            }),
            _ => return None,
        };

        Some((obj, len))
    }

    /// Try to parse a link.
    fn try_parse_link(&self, text: &str) -> Option<(Link, usize)> {
        if !text.starts_with("[[") {
            return None;
        }

        // Find closing ]]
        let close = text.find("]]")?;
        let inner = &text[2..close];

        // Check for description: [[path][description]]
        let (path, description) = if let Some(sep) = inner.find("][") {
            let desc_text = &inner[sep + 2..];
            (&inner[..sep], Some(self.parse_objects(desc_text)))
        } else {
            (inner, None)
        };

        // Determine link type
        let (link_type, link_path) = self.parse_link_path(path);

        Some((
            Link {
                link_type,
                path: link_path,
                description,
            },
            close + 2,
        ))
    }

    /// Parse a link path to determine its type.
    fn parse_link_path(&self, path: &str) -> (LinkType, String) {
        if let Some(rest) = path.strip_prefix("file:") {
            (LinkType::File, rest.to_string())
        } else if let Some(rest) = path.strip_prefix("id:") {
            (LinkType::Id, rest.to_string())
        } else if let Some(rest) = path.strip_prefix('#') {
            (LinkType::CustomId, rest.to_string())
        } else if path.starts_with('(') {
            (LinkType::Coderef, path.to_string())
        } else if let Some(colon_pos) = path.find(':') {
            let protocol = &path[..colon_pos];
            if protocol
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '+' || c == '-' || c == '.')
            {
                (LinkType::Protocol(protocol.to_string()), path.to_string())
            } else {
                (LinkType::Fuzzy, path.to_string())
            }
        } else {
            (LinkType::Fuzzy, path.to_string())
        }
    }

    /// Try to parse a footnote reference.
    fn try_parse_footnote_ref(&self, text: &str) -> Option<(FootnoteReference, usize)> {
        if !text.starts_with("[fn:") {
            return None;
        }

        let close = text.find(']')?;
        let inner = &text[4..close];

        // [fn:label] or [fn:label:definition] or [fn::definition]
        let (label, definition) = if let Some(colon_pos) = inner.find(':') {
            let label_part = &inner[..colon_pos];
            let def_part = &inner[colon_pos + 1..];
            (
                if label_part.is_empty() {
                    None
                } else {
                    Some(label_part.to_string())
                },
                if def_part.is_empty() {
                    None
                } else {
                    Some(self.parse_objects(def_part))
                },
            )
        } else {
            (Some(inner.to_string()), None)
        };

        Some((FootnoteReference { label, definition }, close + 1))
    }

    /// Try to parse a statistics cookie.
    fn try_parse_statistics_cookie(&self, text: &str) -> Option<(StatisticsCookie, usize)> {
        if !text.starts_with('[') {
            return None;
        }

        let close = text.find(']')?;
        let inner = &text[1..close];

        // [n/m] or [n%]
        if let Some(num_str) = inner.strip_suffix('%') {
            let num = if num_str.is_empty() {
                None
            } else {
                num_str.parse().ok()
            };
            Some((StatisticsCookie::Percent(num), close + 1))
        } else if inner.contains('/') {
            let parts: Vec<&str> = inner.split('/').collect();
            if parts.len() == 2 {
                let num = if parts[0].is_empty() {
                    None
                } else {
                    parts[0].parse().ok()
                };
                let denom = if parts[1].is_empty() {
                    None
                } else {
                    parts[1].parse().ok()
                };
                Some((
                    StatisticsCookie::Fraction {
                        numerator: num,
                        denominator: denom,
                    },
                    close + 1,
                ))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Try to parse a timestamp.
    fn try_parse_timestamp(&self, text: &str) -> Option<(Timestamp, usize)> {
        let (ts, remaining) = self.parse_timestamp_from_str(text)?;
        let consumed = text.len() - remaining.len();
        Some((ts, consumed))
    }

    /// Parse a timestamp from a string, returning the timestamp and remaining text.
    fn parse_timestamp_from_str<'b>(&self, text: &'b str) -> Option<(Timestamp, &'b str)> {
        let active = text.starts_with('<');
        let inactive = text.starts_with('[');

        if !active && !inactive {
            return None;
        }

        let open_bracket = if active { '<' } else { '[' };
        let close_bracket = if active { '>' } else { ']' };

        // Find the closing bracket
        let close_pos = text.find(close_bracket)?;
        let inner = &text[1..close_pos];

        // Parse the date/time
        let (start, repeater, warning) = self.parse_datetime_with_modifiers(inner)?;

        let mut remaining = &text[close_pos + 1..];
        let mut end = None;
        let mut ts_type = if active {
            TimestampType::Active
        } else {
            TimestampType::Inactive
        };

        // Check for range: --<date> or --[date]
        if remaining.starts_with("--") {
            remaining = &remaining[2..];
            if remaining.starts_with(open_bracket)
                && let Some(end_close) = remaining.find(close_bracket)
            {
                let end_inner = &remaining[1..end_close];
                if let Some((end_dt, _, _)) = self.parse_datetime_with_modifiers(end_inner) {
                    end = Some(end_dt);
                    ts_type = if active {
                        TimestampType::ActiveRange
                    } else {
                        TimestampType::InactiveRange
                    };
                    remaining = &remaining[end_close + 1..];
                }
            }
        }

        Some((
            Timestamp {
                timestamp_type: ts_type,
                start,
                end,
                repeater,
                warning,
            },
            remaining,
        ))
    }

    /// Parse a datetime with optional repeater and warning.
    fn parse_datetime_with_modifiers(
        &self,
        text: &str,
    ) -> Option<(DateTime, Option<Repeater>, Option<WarningDelay>)> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        // Parse date: YYYY-MM-DD
        let date_parts: Vec<&str> = parts[0].split('-').collect();
        if date_parts.len() != 3 {
            return None;
        }

        let year: i32 = date_parts[0].parse().ok()?;
        let month: u8 = date_parts[1].parse().ok()?;
        let day: u8 = date_parts[2].parse().ok()?;

        let mut datetime = DateTime {
            year,
            month,
            day,
            dayname: None,
            hour: None,
            minute: None,
            end_hour: None,
            end_minute: None,
        };

        let mut repeater = None;
        let mut warning = None;
        let mut idx = 1;

        // Optional day name
        if idx < parts.len() && parts[idx].chars().all(|c| c.is_alphabetic()) {
            datetime.dayname = Some(parts[idx].to_string());
            idx += 1;
        }

        // Optional time
        if idx < parts.len() && parts[idx].contains(':') {
            let time_part = parts[idx];
            // Check for time range HH:MM-HH:MM
            if let Some(dash_pos) = time_part.find('-') {
                let start_time = &time_part[..dash_pos];
                let end_time = &time_part[dash_pos + 1..];
                if let Some((h, m)) = self.parse_time(start_time) {
                    datetime.hour = Some(h);
                    datetime.minute = Some(m);
                }
                if let Some((h, m)) = self.parse_time(end_time) {
                    datetime.end_hour = Some(h);
                    datetime.end_minute = Some(m);
                }
            } else if let Some((h, m)) = self.parse_time(time_part) {
                datetime.hour = Some(h);
                datetime.minute = Some(m);
            }
            idx += 1;
        }

        // Parse repeater and warning delay
        while idx < parts.len() {
            let part = parts[idx];
            if part.starts_with('.') || part.starts_with('+') {
                repeater = self.parse_repeater(part);
            } else if part.starts_with('-') {
                warning = self.parse_warning_delay(part);
            }
            idx += 1;
        }

        Some((datetime, repeater, warning))
    }

    /// Parse a time string HH:MM.
    fn parse_time(&self, text: &str) -> Option<(u8, u8)> {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 2 {
            return None;
        }
        let hour: u8 = parts[0].parse().ok()?;
        let minute: u8 = parts[1].parse().ok()?;
        Some((hour, minute))
    }

    /// Parse a repeater string.
    fn parse_repeater(&self, text: &str) -> Option<Repeater> {
        let (repeater_type, rest) = if let Some(rest) = text.strip_prefix("++") {
            (RepeaterType::CatchUp, rest)
        } else if let Some(rest) = text.strip_prefix(".+") {
            (RepeaterType::Restart, rest)
        } else if let Some(rest) = text.strip_prefix('+') {
            (RepeaterType::Cumulative, rest)
        } else {
            return None;
        };

        let (value, unit) = self.parse_interval(rest)?;
        Some(Repeater {
            repeater_type,
            value,
            unit,
        })
    }

    /// Parse a warning delay string.
    fn parse_warning_delay(&self, text: &str) -> Option<WarningDelay> {
        let (warning_type, rest) = if let Some(rest) = text.strip_prefix("--") {
            (WarningType::First, rest)
        } else if let Some(rest) = text.strip_prefix('-') {
            (WarningType::All, rest)
        } else {
            return None;
        };

        let (value, unit) = self.parse_interval(rest)?;
        Some(WarningDelay {
            warning_type,
            value,
            unit,
        })
    }

    /// Parse an interval like "1d", "2w", etc.
    fn parse_interval(&self, text: &str) -> Option<(u32, TimeUnit)> {
        if text.is_empty() {
            return None;
        }

        let num_end = text
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len());
        if num_end == 0 {
            return None;
        }

        let value: u32 = text[..num_end].parse().ok()?;
        let unit_char = text.chars().nth(num_end)?;

        let unit = match unit_char {
            'h' => TimeUnit::Hour,
            'd' => TimeUnit::Day,
            'w' => TimeUnit::Week,
            'm' => TimeUnit::Month,
            'y' => TimeUnit::Year,
            _ => return None,
        };

        Some((value, unit))
    }

    /// Consolidate adjacent text objects.
    fn consolidate_text_objects(&self, mut objects: Vec<Object>) -> Vec<Object> {
        if objects.len() < 2 {
            return objects;
        }

        let mut result = Vec::with_capacity(objects.len());
        let mut current_text: Option<String> = None;

        for obj in objects.drain(..) {
            match obj {
                Object::Text(t) => {
                    if let Some(ref mut text) = current_text {
                        text.push_str(&t.value);
                    } else {
                        current_text = Some(t.value);
                    }
                }
                other => {
                    if let Some(text) = current_text.take() {
                        result.push(Object::Text(Text { value: text }));
                    }
                    result.push(other);
                }
            }
        }

        if let Some(text) = current_text {
            result.push(Object::Text(Text { value: text }));
        }

        result
    }
}

/// Parse an Org-mode document from a string.
pub fn parse(input: &str) -> Result<Document, ParseError> {
    Parser::new(input).parse()
}

/// Parse an Org-mode document with custom configuration.
pub fn parse_with_config(input: &str, config: ParseConfig) -> Result<Document, ParseError> {
    Parser::with_config(input, config).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_document() {
        let doc = parse("").unwrap();
        assert!(doc.zeroth_section.is_none());
        assert!(doc.headlines.is_empty());
    }

    #[test]
    fn simple_headline() {
        let doc = parse("* Hello World").unwrap();
        assert_eq!(doc.headlines.len(), 1);
        assert_eq!(doc.headlines[0].level, 1);
        assert_eq!(doc.headlines[0].title.len(), 1);
        if let Object::Text(t) = &doc.headlines[0].title[0] {
            assert_eq!(t.value, "Hello World");
        } else {
            panic!("expected text object, got {:?}", doc.headlines[0].title[0]);
        }
    }

    #[test]
    fn headline_with_todo() {
        let doc = parse("* TODO Task").unwrap();
        assert!(doc.headlines[0].keyword.is_some());
        let kw = doc.headlines[0].keyword.as_ref().unwrap();
        assert_eq!(kw.keyword, "TODO");
        assert!(!kw.done);
    }

    #[test]
    fn headline_with_done() {
        let doc = parse("* DONE Task").unwrap();
        let kw = doc.headlines[0].keyword.as_ref().unwrap();
        assert_eq!(kw.keyword, "DONE");
        assert!(kw.done);
    }

    #[test]
    fn headline_with_priority() {
        let doc = parse("* [#A] Important").unwrap();
        assert!(doc.headlines[0].priority.is_some());
        assert_eq!(doc.headlines[0].priority.as_ref().unwrap().value, 'A');
    }

    #[test]
    fn headline_with_tags() {
        let doc = parse("* Task :tag1:tag2:").unwrap();
        assert_eq!(doc.headlines[0].tags.len(), 2);
        assert_eq!(doc.headlines[0].tags[0].tag, "tag1");
        assert_eq!(doc.headlines[0].tags[1].tag, "tag2");
    }

    #[test]
    fn headline_with_all_components() {
        let doc = parse("* TODO [#B] COMMENT My Task :work:urgent:").unwrap();
        let h = &doc.headlines[0];
        assert_eq!(h.keyword.as_ref().unwrap().keyword, "TODO");
        assert_eq!(h.priority.as_ref().unwrap().value, 'B');
        assert!(h.commented);
        assert_eq!(h.tags.len(), 2);
    }

    #[test]
    fn nested_headlines() {
        let doc = parse("* Level 1\n** Level 2\n*** Level 3\n** Another Level 2").unwrap();
        assert_eq!(doc.headlines.len(), 1);
        assert_eq!(doc.headlines[0].children.len(), 2);
        assert_eq!(doc.headlines[0].children[0].children.len(), 1);
    }

    #[test]
    fn planning_line() {
        let doc = parse("* Task\nDEADLINE: <2024-01-15>").unwrap();
        assert!(doc.headlines[0].planning.is_some());
        let planning = doc.headlines[0].planning.as_ref().unwrap();
        assert!(planning.deadline.is_some());
        let ts = planning.deadline.as_ref().unwrap();
        assert_eq!(ts.start.year, 2024);
        assert_eq!(ts.start.month, 1);
        assert_eq!(ts.start.day, 15);
    }

    #[test]
    fn property_drawer() {
        let doc = parse("* Task\n:PROPERTIES:\n:ID: 123\n:CUSTOM: value\n:END:").unwrap();
        assert!(doc.headlines[0].properties.is_some());
        let props = doc.headlines[0].properties.as_ref().unwrap();
        assert_eq!(props.properties.len(), 2);
        assert_eq!(props.properties[0].name, "ID");
        assert_eq!(props.properties[0].value, "123");
    }

    #[test]
    fn paragraph_content() {
        let doc = parse("* Headline\nThis is a paragraph.\nWith multiple lines.").unwrap();
        assert!(doc.headlines[0].section.is_some());
        let section = doc.headlines[0].section.as_ref().unwrap();
        assert_eq!(section.elements.len(), 1);
        assert!(matches!(section.elements[0], Element::Paragraph(_)));
    }

    #[test]
    fn bold_markup() {
        let doc = parse("* Test\nThis is *bold* text.").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::Paragraph(p) = &section.elements[0] {
            // Should have: "This is ", bold("bold"), " text."
            assert!(p.contents.iter().any(|o| matches!(o, Object::Bold(_))));
        } else {
            panic!("expected paragraph");
        }
    }

    #[test]
    fn italic_markup() {
        let doc = parse("* Test\nThis is /italic/ text.").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::Paragraph(p) = &section.elements[0] {
            assert!(p.contents.iter().any(|o| matches!(o, Object::Italic(_))));
        } else {
            panic!("expected paragraph");
        }
    }

    #[test]
    fn code_markup() {
        let doc = parse("* Test\nThis is ~code~ text.").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::Paragraph(p) = &section.elements[0] {
            assert!(p.contents.iter().any(|o| matches!(o, Object::Code(_))));
        } else {
            panic!("expected paragraph");
        }
    }

    #[test]
    fn link_parsing() {
        let doc = parse("* Test\nSee [[https://example.com][Example]].").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::Paragraph(p) = &section.elements[0] {
            let link = p.contents.iter().find(|o| matches!(o, Object::Link(_)));
            assert!(link.is_some());
            if let Some(Object::Link(l)) = link {
                assert!(matches!(l.link_type, LinkType::Protocol(_)));
                assert!(l.description.is_some());
            }
        } else {
            panic!("expected paragraph");
        }
    }

    #[test]
    fn source_block() {
        let input = "* Code\n#+BEGIN_SRC rust\nfn main() {}\n#+END_SRC";
        let doc = parse(input).unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::SourceBlock(sb) = &section.elements[0] {
            assert_eq!(sb.language, Some("rust".to_string()));
            assert!(sb.contents.contains("fn main()"));
        } else {
            panic!("expected source block, got {:?}", section.elements[0]);
        }
    }

    #[test]
    fn unordered_list() {
        let doc = parse("* List\n- Item 1\n- Item 2").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::PlainList(pl) = &section.elements[0] {
            assert_eq!(pl.list_type, ListType::Unordered);
            assert_eq!(pl.items.len(), 2);
        } else {
            panic!("expected plain list, got {:?}", section.elements[0]);
        }
    }

    #[test]
    fn ordered_list() {
        let doc = parse("* List\n1. First\n2. Second").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::PlainList(pl) = &section.elements[0] {
            assert_eq!(pl.list_type, ListType::Ordered);
            assert_eq!(pl.items.len(), 2);
        } else {
            panic!("expected plain list, got {:?}", section.elements[0]);
        }
    }

    #[test]
    fn checkbox_list() {
        let doc = parse("* Tasks\n- [ ] Todo\n- [X] Done\n- [-] Partial").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::PlainList(pl) = &section.elements[0] {
            assert_eq!(pl.items[0].checkbox, Some(Checkbox::Unchecked));
            assert_eq!(pl.items[1].checkbox, Some(Checkbox::Checked));
            assert_eq!(pl.items[2].checkbox, Some(Checkbox::Partial));
        } else {
            panic!("expected plain list");
        }
    }

    #[test]
    fn horizontal_rule() {
        let doc = parse("* Section\n-----\nAfter rule").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        assert!(
            section
                .elements
                .iter()
                .any(|e| matches!(e, Element::HorizontalRule(_)))
        );
    }

    #[test]
    fn keyword() {
        let doc = parse("#+TITLE: My Document\n* Headline").unwrap();
        assert!(doc.zeroth_section.is_some());
        let section = doc.zeroth_section.as_ref().unwrap();
        if let Element::Keyword(kw) = &section.elements[0] {
            assert_eq!(kw.key, "TITLE");
            assert_eq!(kw.value, "My Document");
        } else {
            panic!("expected keyword");
        }
    }

    #[test]
    fn timestamp_with_time() {
        let doc = parse("* Meeting\nDEADLINE: <2024-01-15 Mon 10:30>").unwrap();
        let planning = doc.headlines[0].planning.as_ref().unwrap();
        let ts = planning.deadline.as_ref().unwrap();
        assert_eq!(ts.start.hour, Some(10));
        assert_eq!(ts.start.minute, Some(30));
        assert_eq!(ts.start.dayname, Some("Mon".to_string()));
    }

    #[test]
    fn timestamp_with_repeater() {
        let doc = parse("* Recurring\nDEADLINE: <2024-01-15 +1w>").unwrap();
        let planning = doc.headlines[0].planning.as_ref().unwrap();
        let ts = planning.deadline.as_ref().unwrap();
        assert!(ts.repeater.is_some());
        let rep = ts.repeater.as_ref().unwrap();
        assert_eq!(rep.repeater_type, RepeaterType::Cumulative);
        assert_eq!(rep.value, 1);
        assert_eq!(rep.unit, TimeUnit::Week);
    }

    #[test]
    fn inactive_timestamp() {
        let doc = parse("* Note\nCreated [2024-01-15]").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::Paragraph(p) = &section.elements[0] {
            let ts = p
                .contents
                .iter()
                .find(|o| matches!(o, Object::Timestamp(_)));
            assert!(ts.is_some());
            if let Some(Object::Timestamp(t)) = ts {
                assert_eq!(t.timestamp_type, TimestampType::Inactive);
            }
        } else {
            panic!("expected paragraph");
        }
    }

    #[test]
    fn drawer() {
        let doc = parse("* Headline\n:LOGBOOK:\nSome log content\n:END:").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::Drawer(d) = &section.elements[0] {
            assert_eq!(d.name, "LOGBOOK");
        } else {
            panic!("expected drawer, got {:?}", section.elements[0]);
        }
    }

    #[test]
    fn comment_line() {
        let doc = parse("* Headline\n# This is a comment").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::Comment(c) = &section.elements[0] {
            assert_eq!(c.value, "This is a comment");
        } else {
            panic!("expected comment");
        }
    }

    #[test]
    fn fixed_width() {
        let doc = parse("* Example\n: fixed width\n: content").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::FixedWidth(fw) = &section.elements[0] {
            assert!(fw.value.contains("fixed width"));
        } else {
            panic!("expected fixed width");
        }
    }

    #[test]
    fn footnote_reference() {
        let doc = parse("* Text\nSee footnote[fn:1].").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::Paragraph(p) = &section.elements[0] {
            let fnref = p
                .contents
                .iter()
                .find(|o| matches!(o, Object::FootnoteReference(_)));
            assert!(fnref.is_some());
            if let Some(Object::FootnoteReference(f)) = fnref {
                assert_eq!(f.label, Some("1".to_string()));
            }
        } else {
            panic!("expected paragraph");
        }
    }

    #[test]
    fn statistics_cookie_fraction() {
        let doc = parse("* Tasks [2/5]").unwrap();
        let h = &doc.headlines[0];
        let cookie = h
            .title
            .iter()
            .find(|o| matches!(o, Object::StatisticsCookie(_)));
        assert!(cookie.is_some());
        if let Some(Object::StatisticsCookie(StatisticsCookie::Fraction {
            numerator,
            denominator,
        })) = cookie
        {
            assert_eq!(*numerator, Some(2));
            assert_eq!(*denominator, Some(5));
        } else {
            panic!("expected fraction cookie");
        }
    }

    #[test]
    fn statistics_cookie_percent() {
        let doc = parse("* Progress [40%]").unwrap();
        let h = &doc.headlines[0];
        let cookie = h
            .title
            .iter()
            .find(|o| matches!(o, Object::StatisticsCookie(_)));
        if let Some(Object::StatisticsCookie(StatisticsCookie::Percent(p))) = cookie {
            assert_eq!(*p, Some(40));
        } else {
            panic!("expected percent cookie");
        }
    }

    #[test]
    fn quote_block() {
        let input = "* Quote\n#+BEGIN_QUOTE\nTo be or not to be.\n#+END_QUOTE";
        let doc = parse(input).unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        assert!(matches!(section.elements[0], Element::QuoteBlock(_)));
    }

    #[test]
    fn example_block() {
        let input = "* Example\n#+BEGIN_EXAMPLE\nExample text\n#+END_EXAMPLE";
        let doc = parse(input).unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::ExampleBlock(eb) = &section.elements[0] {
            assert!(eb.contents.contains("Example text"));
        } else {
            panic!("expected example block");
        }
    }

    #[test]
    fn multiple_planning_keywords() {
        let doc = parse("* Task\nSCHEDULED: <2024-01-10> DEADLINE: <2024-01-15>").unwrap();
        let planning = doc.headlines[0].planning.as_ref().unwrap();
        assert!(planning.scheduled.is_some());
        assert!(planning.deadline.is_some());
        assert_eq!(planning.scheduled.as_ref().unwrap().start.day, 10);
        assert_eq!(planning.deadline.as_ref().unwrap().start.day, 15);
    }

    #[test]
    fn zeroth_section() {
        let doc = parse("Some initial content.\n\n* First Headline").unwrap();
        assert!(doc.zeroth_section.is_some());
        let section = doc.zeroth_section.as_ref().unwrap();
        assert!(!section.elements.is_empty());
    }

    #[test]
    fn description_list() {
        let doc = parse("* Definitions\n- Term :: Definition here").unwrap();
        let section = doc.headlines[0].section.as_ref().unwrap();
        if let Element::PlainList(pl) = &section.elements[0] {
            assert_eq!(pl.list_type, ListType::Description);
            assert!(pl.items[0].tag.is_some());
        } else {
            panic!("expected plain list");
        }
    }
}
