//! Display implementations for rendering the AST as canonical Org-mode text.

use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result;
use std::fmt::Write;

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
use crate::NodeProperty;
use crate::Object;
use crate::Paragraph;
use crate::PlainList;
use crate::Planning;
use crate::PropertyDrawer;
use crate::QuoteBlock;
use crate::Repeater;
use crate::RepeaterType;
use crate::Section;
use crate::SourceBlock;
use crate::StatisticsCookie;
use crate::StrikeThrough;
use crate::Text;
use crate::TimeUnit;
use crate::Timestamp;
use crate::TimestampType;
use crate::Underline;
use crate::Verbatim;
use crate::VerseBlock;
use crate::WarningDelay;
use crate::WarningType;

////////////////////////////////////////////// Document //////////////////////////////////////////////

impl Display for Document {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let Some(ref section) = self.zeroth_section {
            write!(f, "{}", section)?;
        }
        for headline in &self.headlines {
            write!(f, "{}", headline)?;
        }
        Ok(())
    }
}

////////////////////////////////////////////// Headline //////////////////////////////////////////////

impl Display for Headline {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        // Stars
        for _ in 0..self.level {
            f.write_char('*')?;
        }

        // Space after stars
        f.write_char(' ')?;

        // TODO keyword
        if let Some(ref kw) = self.keyword {
            write!(f, "{} ", kw.keyword())?;
        }

        // Priority
        if let Some(ref pri) = self.priority {
            write!(f, "[#{}] ", pri.value())?;
        }

        // COMMENT
        if self.commented {
            f.write_str("COMMENT ")?;
        }

        // Title
        for obj in &self.title {
            write!(f, "{}", obj)?;
        }

        // Tags
        if !self.tags.is_empty() {
            f.write_str(" :")?;
            for tag in &self.tags {
                write!(f, "{}:", tag.tag())?;
            }
        }

        f.write_char('\n')?;

        // Planning
        if let Some(ref planning) = self.planning {
            write!(f, "{}", planning)?;
        }

        // Property drawer
        if let Some(ref props) = self.properties {
            write!(f, "{}", props)?;
        }

        // Section
        if let Some(ref section) = self.section {
            write!(f, "{}", section)?;
        }

        // Children
        for child in &self.children {
            write!(f, "{}", child)?;
        }

        Ok(())
    }
}

////////////////////////////////////////////// Planning //////////////////////////////////////////////

impl Display for Planning {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut first = true;

        if let Some(ref closed) = self.closed {
            write!(f, "CLOSED: {}", closed)?;
            first = false;
        }

        if let Some(ref deadline) = self.deadline {
            if !first {
                f.write_char(' ')?;
            }
            write!(f, "DEADLINE: {}", deadline)?;
            first = false;
        }

        if let Some(ref scheduled) = self.scheduled {
            if !first {
                f.write_char(' ')?;
            }
            write!(f, "SCHEDULED: {}", scheduled)?;
        }

        f.write_char('\n')
    }
}

////////////////////////////////////////// PropertyDrawer /////////////////////////////////////////

impl Display for PropertyDrawer {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str(":PROPERTIES:\n")?;
        for prop in &self.properties {
            write!(f, "{}", prop)?;
        }
        f.write_str(":END:\n")
    }
}

impl Display for NodeProperty {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        writeln!(f, ":{}: {}", self.name(), self.value())
    }
}

////////////////////////////////////////////// Section //////////////////////////////////////////////

impl Display for Section {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        for element in &self.elements {
            write!(f, "{}", element)?;
        }
        Ok(())
    }
}

////////////////////////////////////////////// Element //////////////////////////////////////////////

impl Display for Element {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Element::Paragraph(p) => write!(f, "{}", p),
            Element::PlainList(l) => write!(f, "{}", l),
            Element::Drawer(d) => write!(f, "{}", d),
            Element::SourceBlock(b) => write!(f, "{}", b),
            Element::ExampleBlock(b) => write!(f, "{}", b),
            Element::QuoteBlock(b) => write!(f, "{}", b),
            Element::VerseBlock(b) => write!(f, "{}", b),
            Element::CenterBlock(b) => write!(f, "{}", b),
            Element::CommentBlock(b) => write!(f, "{}", b),
            Element::Comment(c) => write!(f, "{}", c),
            Element::Keyword(k) => write!(f, "{}", k),
            Element::HorizontalRule(h) => write!(f, "{}", h),
            Element::FixedWidth(fw) => write!(f, "{}", fw),
            Element::Clock(c) => write!(f, "{}", c),
        }
    }
}

////////////////////////////////////////////// Paragraph /////////////////////////////////////////////

impl Display for Paragraph {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        for obj in &self.contents {
            write!(f, "{}", obj)?;
        }
        f.write_char('\n')
    }
}

////////////////////////////////////////////// PlainList /////////////////////////////////////////////

impl Display for PlainList {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        for item in &self.items {
            write!(f, "{}", item)?;
        }
        Ok(())
    }
}

impl Display for ListItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        // Bullet
        write!(f, "{} ", self.bullet)?;

        // Counter set
        if let Some(counter) = self.counter_set {
            write!(f, "[@{}] ", counter)?;
        }

        // Checkbox
        if let Some(cb) = self.checkbox {
            write!(f, "{} ", cb)?;
        }

        // Tag (for description lists)
        if let Some(ref tag) = self.tag {
            for obj in tag {
                write!(f, "{}", obj)?;
            }
            f.write_str(" :: ")?;
        }

        // Contents
        let mut first = true;
        for element in &self.contents {
            if first {
                // First element inline with bullet
                let content = format!("{}", element);
                // Trim trailing newline for inline display
                f.write_str(content.trim_end())?;
                f.write_char('\n')?;
                first = false;
            } else {
                // Subsequent elements indented
                let content = format!("{}", element);
                for line in content.lines() {
                    f.write_str("  ")?;
                    f.write_str(line)?;
                    f.write_char('\n')?;
                }
            }
        }

        if first {
            // No contents, just newline
            f.write_char('\n')?;
        }

        Ok(())
    }
}

impl Display for Checkbox {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Checkbox::Unchecked => f.write_str("[ ]"),
            Checkbox::Checked => f.write_str("[X]"),
            Checkbox::Partial => f.write_str("[-]"),
        }
    }
}

////////////////////////////////////////////// Drawer ////////////////////////////////////////////////

impl Display for Drawer {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        writeln!(f, ":{}:", self.name())?;
        for element in self.contents() {
            write!(f, "{}", element)?;
        }
        f.write_str(":END:\n")
    }
}

////////////////////////////////////////////// Blocks ////////////////////////////////////////////////

impl Display for SourceBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("#+BEGIN_SRC")?;
        if let Some(ref lang) = self.language {
            write!(f, " {}", lang)?;
        }
        if let Some(ref args) = self.arguments {
            write!(f, " {}", args)?;
        }
        f.write_char('\n')?;
        f.write_str(&self.contents)?;
        if !self.contents.ends_with('\n') {
            f.write_char('\n')?;
        }
        f.write_str("#+END_SRC\n")
    }
}

impl Display for ExampleBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("#+BEGIN_EXAMPLE\n")?;
        f.write_str(&self.contents)?;
        if !self.contents.ends_with('\n') {
            f.write_char('\n')?;
        }
        f.write_str("#+END_EXAMPLE\n")
    }
}

impl Display for QuoteBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("#+BEGIN_QUOTE\n")?;
        for element in &self.contents {
            write!(f, "{}", element)?;
        }
        f.write_str("#+END_QUOTE\n")
    }
}

impl Display for VerseBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("#+BEGIN_VERSE\n")?;
        for obj in &self.contents {
            write!(f, "{}", obj)?;
        }
        if !self.contents.is_empty() {
            f.write_char('\n')?;
        }
        f.write_str("#+END_VERSE\n")
    }
}

impl Display for CenterBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("#+BEGIN_CENTER\n")?;
        for element in &self.contents {
            write!(f, "{}", element)?;
        }
        f.write_str("#+END_CENTER\n")
    }
}

impl Display for CommentBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("#+BEGIN_COMMENT\n")?;
        f.write_str(&self.contents)?;
        if !self.contents.ends_with('\n') {
            f.write_char('\n')?;
        }
        f.write_str("#+END_COMMENT\n")
    }
}

////////////////////////////////////////////// Comment ///////////////////////////////////////////////

impl Display for Comment {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.value.is_empty() {
            f.write_str("#\n")
        } else {
            writeln!(f, "# {}", self.value)
        }
    }
}

////////////////////////////////////////////// Keyword ///////////////////////////////////////////////

impl Display for Keyword {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        writeln!(f, "#+{}: {}", self.key, self.value)
    }
}

////////////////////////////////////////// HorizontalRule ///////////////////////////////////////////

impl Display for HorizontalRule {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("-----\n")
    }
}

//////////////////////////////////////////// FixedWidth /////////////////////////////////////////////

impl Display for FixedWidth {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        for line in self.value.lines() {
            writeln!(f, ": {}", line)?;
        }
        if self.value.is_empty() {
            f.write_str(":\n")?;
        }
        Ok(())
    }
}

////////////////////////////////////////////// Clock /////////////////////////////////////////////////

impl Display for Clock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "CLOCK: {}", self.timestamp)?;
        if let Some(ref dur) = self.duration {
            write!(f, " => {}", dur)?;
        }
        f.write_char('\n')
    }
}

impl Display for Duration {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}:{:02}", self.hours, self.minutes)
    }
}

////////////////////////////////////////////// Object ////////////////////////////////////////////////

impl Display for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Object::Text(t) => write!(f, "{}", t),
            Object::Bold(b) => write!(f, "{}", b),
            Object::Italic(i) => write!(f, "{}", i),
            Object::Underline(u) => write!(f, "{}", u),
            Object::StrikeThrough(s) => write!(f, "{}", s),
            Object::Verbatim(v) => write!(f, "{}", v),
            Object::Code(c) => write!(f, "{}", c),
            Object::Link(l) => write!(f, "{}", l),
            Object::Timestamp(t) => write!(f, "{}", t),
            Object::FootnoteReference(fr) => write!(f, "{}", fr),
            Object::LineBreak(lb) => write!(f, "{}", lb),
            Object::StatisticsCookie(sc) => write!(f, "{}", sc),
        }
    }
}

////////////////////////////////////////////// Text //////////////////////////////////////////////////

impl Display for Text {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str(&self.value)
    }
}

////////////////////////////////////////////// Markup ////////////////////////////////////////////////

impl Display for Bold {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_char('*')?;
        for obj in &self.contents {
            write!(f, "{}", obj)?;
        }
        f.write_char('*')
    }
}

impl Display for Italic {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_char('/')?;
        for obj in &self.contents {
            write!(f, "{}", obj)?;
        }
        f.write_char('/')
    }
}

impl Display for Underline {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_char('_')?;
        for obj in &self.contents {
            write!(f, "{}", obj)?;
        }
        f.write_char('_')
    }
}

impl Display for StrikeThrough {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_char('+')?;
        for obj in &self.contents {
            write!(f, "{}", obj)?;
        }
        f.write_char('+')
    }
}

impl Display for Verbatim {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "={}", self.value)?;
        f.write_char('=')
    }
}

impl Display for Code {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "~{}~", self.value)
    }
}

////////////////////////////////////////////// Link //////////////////////////////////////////////////

impl Display for Link {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("[[")?;

        // Write path with appropriate prefix
        match &self.link_type {
            LinkType::File => write!(f, "file:{}", self.path)?,
            LinkType::Id => write!(f, "id:{}", self.path)?,
            LinkType::CustomId => write!(f, "#{}", self.path)?,
            LinkType::Coderef => f.write_str(&self.path)?,
            LinkType::Fuzzy => f.write_str(&self.path)?,
            LinkType::Protocol(_) => f.write_str(&self.path)?,
        }

        // Description
        if let Some(ref desc) = self.description {
            f.write_str("][")?;
            for obj in desc {
                write!(f, "{}", obj)?;
            }
        }

        f.write_str("]]")
    }
}

////////////////////////////////////////////// Timestamp /////////////////////////////////////////////

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let (open, close) = match self.timestamp_type {
            TimestampType::Active | TimestampType::ActiveRange => ('<', '>'),
            TimestampType::Inactive | TimestampType::InactiveRange => ('[', ']'),
            TimestampType::Diary => ('<', '>'),
        };

        f.write_char(open)?;
        write!(f, "{}", self.start)?;

        if let Some(ref rep) = self.repeater {
            write!(f, " {}", rep)?;
        }

        if let Some(ref warn) = self.warning {
            write!(f, " {}", warn)?;
        }

        f.write_char(close)?;

        // Range end
        if let Some(ref end) = self.end {
            f.write_str("--")?;
            f.write_char(open)?;
            write!(f, "{}", end)?;
            f.write_char(close)?;
        }

        Ok(())
    }
}

impl Display for DateTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)?;

        if let Some(ref dayname) = self.dayname {
            write!(f, " {}", dayname)?;
        }

        if let (Some(hour), Some(minute)) = (self.hour, self.minute) {
            write!(f, " {:02}:{:02}", hour, minute)?;

            // Time range within day
            if let (Some(end_hour), Some(end_minute)) = (self.end_hour, self.end_minute) {
                write!(f, "-{:02}:{:02}", end_hour, end_minute)?;
            }
        }

        Ok(())
    }
}

impl Display for Repeater {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.repeater_type {
            RepeaterType::Cumulative => f.write_char('+')?,
            RepeaterType::CatchUp => f.write_str("++")?,
            RepeaterType::Restart => f.write_str(".+")?,
        }
        write!(f, "{}{}", self.value, self.unit)
    }
}

impl Display for WarningDelay {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.warning_type {
            WarningType::All => f.write_char('-')?,
            WarningType::First => f.write_str("--")?,
        }
        write!(f, "{}{}", self.value, self.unit)
    }
}

impl Display for TimeUnit {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            TimeUnit::Hour => f.write_char('h'),
            TimeUnit::Day => f.write_char('d'),
            TimeUnit::Week => f.write_char('w'),
            TimeUnit::Month => f.write_char('m'),
            TimeUnit::Year => f.write_char('y'),
        }
    }
}

//////////////////////////////////////// FootnoteReference //////////////////////////////////////////

impl Display for FootnoteReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("[fn:")?;

        if let Some(ref label) = self.label {
            f.write_str(label)?;
        }

        if let Some(ref def) = self.definition {
            f.write_char(':')?;
            for obj in def {
                write!(f, "{}", obj)?;
            }
        }

        f.write_char(']')
    }
}

//////////////////////////////////////////// LineBreak //////////////////////////////////////////////

impl Display for LineBreak {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("\\\\\n")
    }
}

//////////////////////////////////////// StatisticsCookie ///////////////////////////////////////////

impl Display for StatisticsCookie {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            StatisticsCookie::Fraction {
                numerator,
                denominator,
            } => {
                f.write_char('[')?;
                if let Some(n) = numerator {
                    write!(f, "{}", n)?;
                }
                f.write_char('/')?;
                if let Some(d) = denominator {
                    write!(f, "{}", d)?;
                }
                f.write_char(']')
            }
            StatisticsCookie::Percent(p) => {
                f.write_char('[')?;
                if let Some(pct) = p {
                    write!(f, "{}", pct)?;
                }
                f.write_str("%]")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Priority;
    use crate::Tag;
    use crate::TodoKeyword;
    use crate::parse;

    #[test]
    fn roundtrip_simple_headline() {
        let input = "* Hello World\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_headline_with_todo() {
        let input = "* TODO Task\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_headline_with_priority() {
        let input = "* [#A] Important\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_headline_with_tags() {
        let input = "* Task :tag1:tag2:\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_nested_headlines() {
        let input = "* Level 1\n** Level 2\n*** Level 3\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_planning() {
        let input = "* Task\nDEADLINE: <2024-01-15>\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_property_drawer() {
        let input = "* Task\n:PROPERTIES:\n:ID: 123\n:END:\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_paragraph() {
        let input = "* Headline\nThis is a paragraph.\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_bold() {
        let input = "* Test\nThis is *bold* text.\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_italic() {
        let input = "* Test\nThis is /italic/ text.\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_code() {
        let input = "* Test\nThis is ~code~ text.\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_link() {
        let input = "* Test\nSee [[https://example.com][Example]].\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_source_block() {
        let input = "* Code\n#+BEGIN_SRC rust\nfn main() {}\n#+END_SRC\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_unordered_list() {
        let input = "* List\n- Item 1\n- Item 2\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_horizontal_rule() {
        let input = "* Section\n-----\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_keyword() {
        let input = "#+TITLE: My Document\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_timestamp_with_time() {
        let input = "* Meeting\nDEADLINE: <2024-01-15 Mon 10:30>\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_timestamp_with_repeater() {
        let input = "* Recurring\nDEADLINE: <2024-01-15 +1w>\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_drawer() {
        let input = "* Headline\n:LOGBOOK:\nSome log content\n:END:\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_comment() {
        let input = "* Headline\n# This is a comment\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_fixed_width() {
        let input = "* Example\n: fixed width\n: content\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_footnote() {
        let input = "* Text\nSee footnote[fn:1].\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_statistics_cookie_fraction() {
        let input = "* Tasks [2/5]\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_statistics_cookie_percent() {
        let input = "* Progress [40%]\n";
        let doc = parse(input).unwrap();
        let output = format!("{}", doc);
        assert_eq!(output, input);
    }

    #[test]
    fn display_checkbox() {
        assert_eq!(format!("{}", Checkbox::Unchecked), "[ ]");
        assert_eq!(format!("{}", Checkbox::Checked), "[X]");
        assert_eq!(format!("{}", Checkbox::Partial), "[-]");
    }

    #[test]
    fn display_time_unit() {
        assert_eq!(format!("{}", TimeUnit::Hour), "h");
        assert_eq!(format!("{}", TimeUnit::Day), "d");
        assert_eq!(format!("{}", TimeUnit::Week), "w");
        assert_eq!(format!("{}", TimeUnit::Month), "m");
        assert_eq!(format!("{}", TimeUnit::Year), "y");
    }

    #[test]
    fn display_duration() {
        let dur = Duration {
            hours: 1,
            minutes: 30,
        };
        assert_eq!(format!("{}", dur), "1:30");

        let dur2 = Duration {
            hours: 10,
            minutes: 5,
        };
        assert_eq!(format!("{}", dur2), "10:05");
    }

    #[test]
    fn display_headline_all_components() {
        let headline = Headline {
            level: 1,
            keyword: Some(TodoKeyword::new("TODO", false).unwrap()),
            priority: Some(Priority::new('A').unwrap()),
            commented: true,
            title: vec![Object::Text(Text {
                value: "Task".to_string(),
            })],
            tags: vec![Tag::new("work").unwrap()],
            planning: None,
            properties: None,
            section: None,
            children: vec![],
        };
        assert_eq!(format!("{}", headline), "* TODO [#A] COMMENT Task :work:\n");
    }
}
