#![deny(missing_docs)]

//! An AST for an Org-mode-like language.
//!
//! This module provides the core document structure for representing Org-mode documents,
//! including headlines, sections, planning information, property drawers, and basic markup.
//!
//! All AST types implement `Display` to render as canonical Org-mode text.
//!
//! Types with validation constraints provide `new()` constructors that validate input.
//! The struct fields remain public for pattern matching, but direct construction bypasses
//! validation.

mod display;
mod parser;
mod tooling;
mod validate;

pub use parser::ParseConfig;
pub use parser::ParseError;
pub use parser::Parser;
pub use parser::parse;
pub use parser::parse_with_config;
pub use tooling::BodyUpdateMode;
pub use tooling::DeleteStrategy;
pub use tooling::InsertPosition;
pub use tooling::NodePath;
pub use tooling::PlanningType;
pub use tooling::ToolError;
pub use tooling::clock_from_datetimes;
pub use tooling::delete_node_by_id;
pub use tooling::ensure_id;
pub use tooling::find_node_path_by_id;
pub use tooling::headline_mut_by_path;
pub use tooling::insert_headline;
pub use tooling::log_clock_by_id;
pub use tooling::refile_node_by_id;
pub use tooling::set_planning_by_id;
pub use tooling::set_priority_by_id;
pub use tooling::set_property_by_id;
pub use tooling::set_state_by_id;
pub use tooling::set_tags_by_id;
pub use tooling::set_title_by_id;
pub use tooling::update_body_by_id;
pub use validate::ValidationError;
pub use validate::validate_day;
pub use validate::validate_drawer_name;
pub use validate::validate_headline_level;
pub use validate::validate_hour;
pub use validate::validate_minute;
pub use validate::validate_month;
pub use validate::validate_priority;
pub use validate::validate_property_name;
pub use validate::validate_property_value;
pub use validate::validate_tag;
pub use validate::validate_todo_keyword;

////////////////////////////////////////////// Document //////////////////////////////////////////////

/// A complete Org-mode document.
///
/// An Org document consists of an optional initial section (content before any headline)
/// followed by zero or more top-level headlines.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Document {
    /// Content appearing before the first headline.
    pub zeroth_section: Option<Section>,
    /// The top-level headlines in the document.
    pub headlines: Vec<Headline>,
}

////////////////////////////////////////////// Headline //////////////////////////////////////////////

/// A headline in an Org document.
///
/// Headlines begin with one or more asterisks followed by optional components:
/// `*+ [KEYWORD] [PRIORITY] TITLE [TAGS]`
///
/// Headlines form a tree structure where nesting is determined by the number of stars.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Headline {
    /// The level of the headline (number of leading asterisks, 1+).
    pub level: u8,
    /// The TODO keyword (e.g., "TODO", "DONE").
    pub keyword: Option<TodoKeyword>,
    /// The priority cookie (e.g., 'A', 'B', 'C').
    pub priority: Option<Priority>,
    /// Whether the headline is commented (starts with "COMMENT").
    pub commented: bool,
    /// The title of the headline as a sequence of objects.
    pub title: Vec<Object>,
    /// Tags associated with the headline.
    pub tags: Vec<Tag>,
    /// The planning information (DEADLINE, SCHEDULED, CLOSED).
    pub planning: Option<Planning>,
    /// The property drawer for this headline.
    pub properties: Option<PropertyDrawer>,
    /// The section content following the headline.
    pub section: Option<Section>,
    /// Child headlines nested under this headline.
    pub children: Vec<Headline>,
}

/// A TODO keyword.
///
/// Org-mode supports configurable TODO keywords. The standard keywords are TODO and DONE.
/// Keywords must contain only uppercase ASCII letters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodoKeyword {
    keyword: String,
    done: bool,
}

impl TodoKeyword {
    /// Create a new TODO keyword with validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the keyword is empty or contains non-uppercase ASCII letters.
    pub fn new(keyword: impl Into<String>, done: bool) -> Result<Self, ValidationError> {
        let keyword = keyword.into();
        validate_todo_keyword(&keyword)?;
        Ok(Self { keyword, done })
    }

    /// Returns the keyword text.
    pub fn keyword(&self) -> &str {
        &self.keyword
    }

    /// Returns whether this keyword represents a done state.
    pub fn is_done(&self) -> bool {
        self.done
    }
}

/// A priority cookie.
///
/// Priorities are single uppercase letters A-Z, typically A-C, where A is highest priority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Priority {
    value: char,
}

impl Priority {
    /// Create a new priority with validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not an uppercase ASCII letter (A-Z).
    pub fn new(value: char) -> Result<Self, ValidationError> {
        validate_priority(value)?;
        Ok(Self { value })
    }

    /// Returns the priority character.
    pub fn value(&self) -> char {
        self.value
    }
}

/// A tag associated with a headline.
///
/// Tags appear at the end of a headline, delimited by colons: `:tag1:tag2:`.
/// Tags must contain only alphanumeric characters, underscores, and @ symbols.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tag {
    tag: String,
}

impl Tag {
    /// Create a new tag with validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the tag is empty or contains invalid characters.
    pub fn new(tag: impl Into<String>) -> Result<Self, ValidationError> {
        let tag = tag.into();
        validate_tag(&tag)?;
        Ok(Self { tag })
    }

    /// Returns the tag text.
    pub fn tag(&self) -> &str {
        &self.tag
    }
}

////////////////////////////////////////////// Planning //////////////////////////////////////////////

/// Planning information for a headline.
///
/// Planning lines immediately follow a headline and contain DEADLINE, SCHEDULED,
/// and/or CLOSED timestamps.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Planning {
    /// The DEADLINE timestamp.
    pub deadline: Option<Timestamp>,
    /// The SCHEDULED timestamp.
    pub scheduled: Option<Timestamp>,
    /// The CLOSED timestamp.
    pub closed: Option<Timestamp>,
}

////////////////////////////////////////// PropertyDrawer /////////////////////////////////////////

/// A property drawer containing node properties.
///
/// Property drawers appear immediately after a headline (and its planning line, if present).
/// They are delimited by `:PROPERTIES:` and `:END:`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PropertyDrawer {
    /// The properties in this drawer.
    pub properties: Vec<NodeProperty>,
}

/// A single node property.
///
/// Properties have the form `:NAME: VALUE` within a property drawer.
/// Property names must contain only alphanumeric characters, underscores, and hyphens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeProperty {
    name: String,
    value: String,
}

impl NodeProperty {
    /// Create a new node property with validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is empty, is "END", or contains invalid characters,
    /// or if the value contains line breaks or NUL bytes.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Result<Self, ValidationError> {
        let name = name.into();
        let value = value.into();
        validate_property_name(&name)?;
        validate_property_value(&value)?;
        Ok(Self { name, value })
    }

    /// Returns the property name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the property value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

////////////////////////////////////////////// Section //////////////////////////////////////////////

/// A section containing paragraph and other elements.
///
/// A section is the content between headlines (or before the first headline).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Section {
    /// The elements in this section.
    pub elements: Vec<Element>,
}

////////////////////////////////////////////// Element //////////////////////////////////////////////

/// An element in a section.
///
/// Elements are the building blocks of sections. They include paragraphs, blocks,
/// and other structural items.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Element {
    /// A paragraph of text.
    Paragraph(Paragraph),
    /// A plain list (bulleted, numbered, or definition).
    PlainList(PlainList),
    /// A drawer.
    Drawer(Drawer),
    /// A source code block.
    SourceBlock(SourceBlock),
    /// An example block.
    ExampleBlock(ExampleBlock),
    /// A quote block.
    QuoteBlock(QuoteBlock),
    /// A verse block.
    VerseBlock(VerseBlock),
    /// A center block.
    CenterBlock(CenterBlock),
    /// A comment block.
    CommentBlock(CommentBlock),
    /// A comment line.
    Comment(Comment),
    /// A keyword (e.g., `#+KEY: value`).
    Keyword(Keyword),
    /// A horizontal rule.
    HorizontalRule(HorizontalRule),
    /// Fixed-width content (lines starting with colon).
    FixedWidth(FixedWidth),
    /// A clock entry.
    Clock(Clock),
}

////////////////////////////////////////////// Paragraph /////////////////////////////////////////////

/// A paragraph of text.
///
/// Paragraphs consist of one or more consecutive lines of text that are not
/// recognized as other element types.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Paragraph {
    /// The objects (text and markup) in this paragraph.
    pub contents: Vec<Object>,
}

////////////////////////////////////////////// PlainList /////////////////////////////////////////////

/// A plain list (bulleted, numbered, or description).
///
/// Lists are sequences of items that begin with a bullet, number, or letter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainList {
    /// The type of list.
    pub list_type: ListType,
    /// The items in the list.
    pub items: Vec<ListItem>,
}

/// The type of a plain list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListType {
    /// Unordered list with bullets (-, +, or *).
    Unordered,
    /// Ordered list with numbers (1., 1)).
    Ordered,
    /// Description list with terms and definitions.
    Description,
}

/// An item in a plain list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListItem {
    /// The bullet or counter string (e.g., "-", "1.", "a)").
    pub bullet: String,
    /// The counter set value, if any (e.g., `[@5]` sets counter to 5).
    pub counter_set: Option<u32>,
    /// The checkbox state, if present.
    pub checkbox: Option<Checkbox>,
    /// The tag for description lists.
    pub tag: Option<Vec<Object>>,
    /// The content of the list item.
    pub contents: Vec<Element>,
}

/// A checkbox state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Checkbox {
    /// Unchecked: `[ ]`.
    Unchecked,
    /// Checked: `[X]`.
    Checked,
    /// Partially checked: `[-]`.
    Partial,
}

////////////////////////////////////////////// Drawer ////////////////////////////////////////////////

/// A drawer (content block delimited by `:NAME:` and `:END:`).
///
/// Drawers hide content by default in Org-mode. They have a name and contain elements.
/// Drawer names must contain only alphanumeric characters, underscores, and hyphens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Drawer {
    name: String,
    contents: Vec<Element>,
}

impl Drawer {
    /// Create a new drawer with validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is empty, is "END" or "PROPERTIES", or contains
    /// invalid characters.
    pub fn new(name: impl Into<String>, contents: Vec<Element>) -> Result<Self, ValidationError> {
        let name = name.into();
        validate_drawer_name(&name)?;
        Ok(Self { name, contents })
    }

    /// Returns the drawer name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the elements inside the drawer.
    pub fn contents(&self) -> &[Element] {
        &self.contents
    }

    /// Returns a mutable reference to the elements inside the drawer.
    pub fn contents_mut(&mut self) -> &mut Vec<Element> {
        &mut self.contents
    }
}

////////////////////////////////////////////// Blocks ////////////////////////////////////////////////

/// A source code block.
///
/// Source blocks are delimited by `#+BEGIN_SRC` and `#+END_SRC`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceBlock {
    /// The language of the source code.
    pub language: Option<String>,
    /// Header arguments for the block.
    pub arguments: Option<String>,
    /// The source code content.
    pub contents: String,
}

/// An example block.
///
/// Example blocks are delimited by `#+BEGIN_EXAMPLE` and `#+END_EXAMPLE`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExampleBlock {
    /// The content of the example block.
    pub contents: String,
}

/// A quote block.
///
/// Quote blocks are delimited by `#+BEGIN_QUOTE` and `#+END_QUOTE`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuoteBlock {
    /// The elements inside the quote block.
    pub contents: Vec<Element>,
}

/// A verse block.
///
/// Verse blocks are delimited by `#+BEGIN_VERSE` and `#+END_VERSE`.
/// They preserve line breaks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VerseBlock {
    /// The objects inside the verse block.
    pub contents: Vec<Object>,
}

/// A center block.
///
/// Center blocks are delimited by `#+BEGIN_CENTER` and `#+END_CENTER`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CenterBlock {
    /// The elements inside the center block.
    pub contents: Vec<Element>,
}

/// A comment block.
///
/// Comment blocks are delimited by `#+BEGIN_COMMENT` and `#+END_COMMENT`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommentBlock {
    /// The raw contents of the comment block.
    pub contents: String,
}

////////////////////////////////////////////// Comment ///////////////////////////////////////////////

/// A comment line.
///
/// Comment lines begin with `#` followed by a space or end of line.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Comment {
    /// The comment text (without the leading `# `).
    pub value: String,
}

////////////////////////////////////////////// Keyword ///////////////////////////////////////////////

/// A keyword.
///
/// Keywords have the form `#+KEY: VALUE`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Keyword {
    /// The keyword name (e.g., "TITLE", "AUTHOR").
    pub key: String,
    /// The keyword value.
    pub value: String,
}

////////////////////////////////////////// HorizontalRule ///////////////////////////////////////////

/// A horizontal rule.
///
/// Horizontal rules consist of 5 or more consecutive hyphens.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HorizontalRule;

//////////////////////////////////////////// FixedWidth /////////////////////////////////////////////

/// Fixed-width content.
///
/// Lines beginning with a colon followed by a space are fixed-width.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FixedWidth {
    /// The fixed-width content.
    pub value: String,
}

////////////////////////////////////////////// Clock /////////////////////////////////////////////////

/// A clock entry.
///
/// Clock entries record time tracking information: `CLOCK: [timestamp]--[timestamp] => duration`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Clock {
    /// The timestamp or timestamp range.
    pub timestamp: Timestamp,
    /// The duration, if the clock is closed.
    pub duration: Option<Duration>,
}

/// A duration in hours and minutes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Duration {
    /// Hours component.
    pub hours: u32,
    /// Minutes component.
    pub minutes: u8,
}

////////////////////////////////////////////// Object ////////////////////////////////////////////////

/// An object (inline element) in Org-mode.
///
/// Objects are the inline elements that compose paragraphs and other text containers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Object {
    /// Plain text.
    Text(Text),
    /// Bold text.
    Bold(Bold),
    /// Italic text.
    Italic(Italic),
    /// Underlined text.
    Underline(Underline),
    /// Strike-through text.
    StrikeThrough(StrikeThrough),
    /// Verbatim text.
    Verbatim(Verbatim),
    /// Code text.
    Code(Code),
    /// A link.
    Link(Link),
    /// A timestamp.
    Timestamp(Timestamp),
    /// A footnote reference.
    FootnoteReference(FootnoteReference),
    /// A line break (double backslash at end of line).
    LineBreak(LineBreak),
    /// A statistics cookie.
    StatisticsCookie(StatisticsCookie),
}

////////////////////////////////////////////// Text //////////////////////////////////////////////////

/// Plain text.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Text {
    /// The text content.
    pub value: String,
}

////////////////////////////////////////////// Markup ////////////////////////////////////////////////

/// Bold text (`*bold*`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Bold {
    /// The objects inside the bold markup.
    pub contents: Vec<Object>,
}

/// Italic text (`/italic/`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Italic {
    /// The objects inside the italic markup.
    pub contents: Vec<Object>,
}

/// Underlined text (`_underline_`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Underline {
    /// The objects inside the underline markup.
    pub contents: Vec<Object>,
}

/// Strike-through text (`+strikethrough+`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StrikeThrough {
    /// The objects inside the strike-through markup.
    pub contents: Vec<Object>,
}

/// Verbatim text (`=verbatim=`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Verbatim {
    /// The verbatim text (not parsed for markup).
    pub value: String,
}

/// Code text (`~code~`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Code {
    /// The code text (not parsed for markup).
    pub value: String,
}

////////////////////////////////////////////// Link //////////////////////////////////////////////////

/// A link.
///
/// Links can be regular links (`[[path][description]]` or `[[path]]`) or angle links (`<URL>`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    /// The link type (e.g., "https", "file", "id").
    pub link_type: LinkType,
    /// The link path/target.
    pub path: String,
    /// The link description (for bracketed links).
    pub description: Option<Vec<Object>>,
}

/// The type of a link.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkType {
    /// A file link.
    File,
    /// An ID link.
    Id,
    /// A custom ID link.
    CustomId,
    /// A code reference link.
    Coderef,
    /// A fuzzy (headline) link.
    Fuzzy,
    /// A protocol link (http, https, mailto, etc.).
    Protocol(String),
}

////////////////////////////////////////////// Timestamp /////////////////////////////////////////////

/// A timestamp.
///
/// Timestamps represent dates and times in various formats.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timestamp {
    /// The type of timestamp.
    pub timestamp_type: TimestampType,
    /// The start date/time.
    pub start: DateTime,
    /// The end date/time (for ranges).
    pub end: Option<DateTime>,
    /// The repeater, if any.
    pub repeater: Option<Repeater>,
    /// The warning delay, if any.
    pub warning: Option<WarningDelay>,
}

/// The type of a timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimestampType {
    /// An active timestamp: `<2024-01-15>`.
    Active,
    /// An inactive timestamp: `[2024-01-15]`.
    Inactive,
    /// An active date range: `<2024-01-15>--<2024-01-16>`.
    ActiveRange,
    /// An inactive date range: `[2024-01-15]--[2024-01-16]`.
    InactiveRange,
    /// A diary sexp timestamp.
    Diary,
}

/// A date and optional time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DateTime {
    /// Year.
    pub year: i32,
    /// Month (1-12).
    pub month: u8,
    /// Day (1-31).
    pub day: u8,
    /// Day of week name (e.g., "Mon", "Tue").
    pub dayname: Option<String>,
    /// Hour (0-23).
    pub hour: Option<u8>,
    /// Minute (0-59).
    pub minute: Option<u8>,
    /// End hour for time ranges within a day.
    pub end_hour: Option<u8>,
    /// End minute for time ranges within a day.
    pub end_minute: Option<u8>,
}

/// A repeater on a timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Repeater {
    /// The repeater type.
    pub repeater_type: RepeaterType,
    /// The numeric value.
    pub value: u32,
    /// The unit.
    pub unit: TimeUnit,
}

/// The type of repeater.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepeaterType {
    /// Cumulative repeater: `+`.
    Cumulative,
    /// Catch-up repeater: `++`.
    CatchUp,
    /// Restart repeater: `.+`.
    Restart,
}

/// A warning delay on a timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WarningDelay {
    /// The warning type.
    pub warning_type: WarningType,
    /// The numeric value.
    pub value: u32,
    /// The unit.
    pub unit: TimeUnit,
}

/// The type of warning delay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WarningType {
    /// All warnings: `-`.
    All,
    /// First warning only: `--`.
    First,
}

/// A time unit for repeaters and warnings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeUnit {
    /// Hour.
    Hour,
    /// Day.
    Day,
    /// Week.
    Week,
    /// Month.
    Month,
    /// Year.
    Year,
}

//////////////////////////////////////// FootnoteReference //////////////////////////////////////////

/// A footnote reference.
///
/// Footnote references are `[fn:label]`, `[fn:label:definition]`, or `[fn::definition]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FootnoteReference {
    /// The footnote label, if any.
    pub label: Option<String>,
    /// An inline definition, if any.
    pub definition: Option<Vec<Object>>,
}

//////////////////////////////////////////// LineBreak //////////////////////////////////////////////

/// An explicit line break (`\\` at end of line).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LineBreak;

//////////////////////////////////////// StatisticsCookie ///////////////////////////////////////////

/// A statistics cookie.
///
/// Statistics cookies show completion status: `[2/5]` or `[40%]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatisticsCookie {
    /// A fraction cookie: `[num/total]`.
    Fraction {
        /// Number complete.
        numerator: Option<u32>,
        /// Total number.
        denominator: Option<u32>,
    },
    /// A percentage cookie: `[percent%]`.
    Percent(Option<u32>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_document_is_empty() {
        let doc = Document::default();
        assert!(doc.zeroth_section.is_none());
        assert!(doc.headlines.is_empty());
    }

    #[test]
    fn headline_with_all_fields() {
        let headline = Headline {
            level: 1,
            keyword: Some(TodoKeyword::new("TODO", false).unwrap()),
            priority: Some(Priority::new('A').unwrap()),
            commented: false,
            title: vec![Object::Text(Text {
                value: "Test headline".to_string(),
            })],
            tags: vec![Tag::new("tag1").unwrap(), Tag::new("tag2").unwrap()],
            planning: Some(Planning {
                deadline: Some(Timestamp {
                    timestamp_type: TimestampType::Active,
                    start: DateTime {
                        year: 2024,
                        month: 1,
                        day: 15,
                        dayname: Some("Mon".to_string()),
                        hour: None,
                        minute: None,
                        end_hour: None,
                        end_minute: None,
                    },
                    end: None,
                    repeater: None,
                    warning: None,
                }),
                scheduled: None,
                closed: None,
            }),
            properties: Some(PropertyDrawer {
                properties: vec![NodeProperty::new("ID", "123").unwrap()],
            }),
            section: Some(Section {
                elements: vec![Element::Paragraph(Paragraph {
                    contents: vec![Object::Text(Text {
                        value: "Some content".to_string(),
                    })],
                })],
            }),
            children: vec![],
        };
        assert_eq!(headline.level, 1);
        assert!(headline.keyword.is_some());
        assert!(headline.priority.is_some());
    }

    #[test]
    fn text_markup_variants() {
        let bold = Object::Bold(Bold {
            contents: vec![Object::Text(Text {
                value: "bold".to_string(),
            })],
        });
        let italic = Object::Italic(Italic {
            contents: vec![Object::Text(Text {
                value: "italic".to_string(),
            })],
        });
        let code = Object::Code(Code {
            value: "code".to_string(),
        });
        let verbatim = Object::Verbatim(Verbatim {
            value: "verbatim".to_string(),
        });

        // Verify they can be constructed and compared
        assert_ne!(bold, italic);
        assert_ne!(code, verbatim);
    }

    #[test]
    fn plain_list_types() {
        let unordered = PlainList {
            list_type: ListType::Unordered,
            items: vec![ListItem {
                bullet: "-".to_string(),
                counter_set: None,
                checkbox: Some(Checkbox::Checked),
                tag: None,
                contents: vec![],
            }],
        };
        let ordered = PlainList {
            list_type: ListType::Ordered,
            items: vec![ListItem {
                bullet: "1.".to_string(),
                counter_set: None,
                checkbox: None,
                tag: None,
                contents: vec![],
            }],
        };
        let description = PlainList {
            list_type: ListType::Description,
            items: vec![ListItem {
                bullet: "-".to_string(),
                counter_set: None,
                checkbox: None,
                tag: Some(vec![Object::Text(Text {
                    value: "term".to_string(),
                })]),
                contents: vec![],
            }],
        };

        assert_eq!(unordered.list_type, ListType::Unordered);
        assert_eq!(ordered.list_type, ListType::Ordered);
        assert_eq!(description.list_type, ListType::Description);
    }

    #[test]
    fn validation_rejects_invalid_todo_keyword() {
        assert!(TodoKeyword::new("todo", false).is_err());
        assert!(TodoKeyword::new("", false).is_err());
        assert!(TodoKeyword::new("TO DO", false).is_err());
    }

    #[test]
    fn validation_rejects_invalid_priority() {
        assert!(Priority::new('a').is_err());
        assert!(Priority::new('1').is_err());
    }

    #[test]
    fn validation_rejects_invalid_tag() {
        assert!(Tag::new("").is_err());
        assert!(Tag::new("tag-with-dash").is_err());
        assert!(Tag::new("tag:colon").is_err());
    }

    #[test]
    fn validation_rejects_invalid_property_name() {
        assert!(NodeProperty::new("", "value").is_err());
        assert!(NodeProperty::new("END", "value").is_err());
        assert!(NodeProperty::new("name:colon", "value").is_err());
    }

    #[test]
    fn validation_rejects_invalid_drawer_name() {
        assert!(Drawer::new("", vec![]).is_err());
        assert!(Drawer::new("END", vec![]).is_err());
        assert!(Drawer::new("PROPERTIES", vec![]).is_err());
    }

    #[test]
    fn timestamp_with_repeater_and_warning() {
        let ts = Timestamp {
            timestamp_type: TimestampType::Active,
            start: DateTime {
                year: 2024,
                month: 1,
                day: 15,
                dayname: Some("Mon".to_string()),
                hour: Some(10),
                minute: Some(30),
                end_hour: None,
                end_minute: None,
            },
            end: None,
            repeater: Some(Repeater {
                repeater_type: RepeaterType::Cumulative,
                value: 1,
                unit: TimeUnit::Week,
            }),
            warning: Some(WarningDelay {
                warning_type: WarningType::All,
                value: 3,
                unit: TimeUnit::Day,
            }),
        };

        assert_eq!(ts.timestamp_type, TimestampType::Active);
        assert!(ts.repeater.is_some());
        assert!(ts.warning.is_some());
    }

    #[test]
    fn link_types() {
        let file_link = Link {
            link_type: LinkType::File,
            path: "./file.org".to_string(),
            description: None,
        };
        let url_link = Link {
            link_type: LinkType::Protocol("https".to_string()),
            path: "//example.com".to_string(),
            description: Some(vec![Object::Text(Text {
                value: "Example".to_string(),
            })]),
        };

        assert_eq!(file_link.link_type, LinkType::File);
        matches!(url_link.link_type, LinkType::Protocol(ref p) if p == "https");
    }

    #[test]
    fn statistics_cookies() {
        let fraction = StatisticsCookie::Fraction {
            numerator: Some(2),
            denominator: Some(5),
        };
        let percent = StatisticsCookie::Percent(Some(40));

        matches!(
            fraction,
            StatisticsCookie::Fraction {
                numerator: Some(2),
                denominator: Some(5)
            }
        );
        matches!(percent, StatisticsCookie::Percent(Some(40)));
    }

    #[test]
    fn source_block_construction() {
        let block = SourceBlock {
            language: Some("rust".to_string()),
            arguments: Some(":results output".to_string()),
            contents: "fn main() {}".to_string(),
        };

        assert_eq!(block.language, Some("rust".to_string()));
        assert!(!block.contents.is_empty());
    }
}
