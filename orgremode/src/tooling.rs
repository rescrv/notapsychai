//! Tooling for programmatic transformations of Org-mode documents.

use std::collections::HashSet;
use std::fmt;

use uuid::Uuid;

use crate::DateTime;
use crate::Document;
use crate::Drawer;
use crate::Element;
use crate::Headline;
use crate::NodeProperty;
use crate::Object;
use crate::ParseError;
use crate::Planning;
use crate::PropertyDrawer;
use crate::Section;
use crate::Tag;
use crate::Timestamp;
use crate::TimestampType;
use crate::TodoKeyword;
use crate::ValidationError;
use crate::parse;
use crate::validate_property_name;
use crate::validate_property_value;

/// A path to a headline in the document tree, expressed as indices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodePath(pub Vec<usize>);

/// Errors returned by tooling operations.
#[derive(Debug)]
pub enum ToolError {
    /// No node matched the requested identifier.
    NodeNotFound(String),
    /// Invalid operation for the requested transform.
    InvalidOperation(String),
    /// Parsing failed while interpreting input content.
    Parse(ParseError),
    /// Validation failed while constructing AST nodes.
    Validation(ValidationError),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::NodeNotFound(id) => write!(f, "node not found: {id}"),
            ToolError::InvalidOperation(msg) => write!(f, "invalid operation: {msg}"),
            ToolError::Parse(err) => write!(f, "parse error: {err}"),
            ToolError::Validation(err) => write!(f, "validation error: {err}"),
        }
    }
}

impl std::error::Error for ToolError {}

impl From<ParseError> for ToolError {
    fn from(err: ParseError) -> Self {
        ToolError::Parse(err)
    }
}

impl From<ValidationError> for ToolError {
    fn from(err: ValidationError) -> Self {
        ToolError::Validation(err)
    }
}

/// How to update the body of a node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyUpdateMode {
    /// Replace the existing section content.
    Replace,
    /// Append to the existing section content.
    Append,
    /// Prepend to the existing section content.
    Prepend,
}

/// How to delete a node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteStrategy {
    /// Delete the node and all descendants.
    Cascade,
    /// Delete the node and promote its children to the parent.
    PromoteChildren,
}

/// Where to insert a node relative to siblings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InsertPosition {
    /// Insert as the first child.
    Prepend,
    /// Insert as the last child.
    Append,
    /// Insert at a specific index.
    Index(usize),
}

/// Which planning slot to update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanningType {
    /// SCHEDULED timestamp.
    Scheduled,
    /// DEADLINE timestamp.
    Deadline,
    /// CLOSED timestamp.
    Closed,
}

/// Find a node by its `:ID:` property and return its path.
pub fn find_node_path_by_id(doc: &Document, id: &str) -> Option<NodePath> {
    find_path_in_headlines(&doc.headlines, id, &mut Vec::new()).map(NodePath)
}

/// Update the body section for a node, with headline sanitization.
pub fn update_body_by_id(
    doc: &mut Document,
    id: &str,
    content: &str,
    mode: BodyUpdateMode,
) -> Result<(), ToolError> {
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;

    let sanitized = sanitize_body_content(content);
    let new_section = parse_section_from_body(&sanitized)?;

    match mode {
        BodyUpdateMode::Replace => {
            headline.section = new_section;
        }
        BodyUpdateMode::Append => {
            if let Some(mut section) = new_section {
                if let Some(existing) = headline.section.as_mut() {
                    existing.elements.append(&mut section.elements);
                } else {
                    headline.section = Some(section);
                }
            }
        }
        BodyUpdateMode::Prepend => {
            if let Some(mut section) = new_section {
                if let Some(existing) = headline.section.as_mut() {
                    let mut elements = Vec::new();
                    elements.append(&mut section.elements);
                    elements.append(&mut existing.elements);
                    existing.elements = elements;
                } else {
                    headline.section = Some(section);
                }
            }
        }
    }

    Ok(())
}

/// Delete a node identified by ID.
pub fn delete_node_by_id(
    doc: &mut Document,
    id: &str,
    strategy: DeleteStrategy,
) -> Result<(), ToolError> {
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;

    let (siblings, index, parent_level) = parent_vec_and_level_mut(&mut doc.headlines, &path.0, 0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;

    let removed = siblings.remove(index);

    if matches!(strategy, DeleteStrategy::PromoteChildren) && !removed.children.is_empty() {
        let mut promoted = removed.children;
        let target_level = parent_level.saturating_add(1);
        for child in &mut promoted {
            let delta = target_level as i16 - child.level as i16;
            adjust_levels(child, delta)?;
        }
        siblings.splice(index..index, promoted);
    }

    Ok(())
}

/// Move a node to a new parent and position.
pub fn refile_node_by_id(
    doc: &mut Document,
    node_id: &str,
    new_parent_id: Option<&str>,
    position: InsertPosition,
) -> Result<(), ToolError> {
    let source_path = find_node_path_by_id(doc, node_id)
        .ok_or_else(|| ToolError::NodeNotFound(node_id.into()))?;

    if let Some(parent_id) = new_parent_id {
        let dest_path = find_node_path_by_id(doc, parent_id)
            .ok_or_else(|| ToolError::NodeNotFound(parent_id.into()))?;
        if path_is_prefix(&source_path.0, &dest_path.0) {
            return Err(ToolError::InvalidOperation(
                "cannot move a node into its own subtree".to_string(),
            ));
        }
    }

    let (siblings, index, _parent_level) =
        parent_vec_and_level_mut(&mut doc.headlines, &source_path.0, 0)
            .ok_or_else(|| ToolError::NodeNotFound(node_id.into()))?;
    let mut node = siblings.remove(index);

    let (dest_parent_level, dest_children) = match new_parent_id {
        Some(parent_id) => {
            let dest_path = find_node_path_by_id(doc, parent_id)
                .ok_or_else(|| ToolError::NodeNotFound(parent_id.into()))?;
            let parent_level = headline_by_path(&doc.headlines, &dest_path.0)
                .ok_or_else(|| ToolError::NodeNotFound(parent_id.into()))?
                .level;
            let dest_children = children_vec_mut(&mut doc.headlines, Some(&dest_path))
                .ok_or_else(|| ToolError::NodeNotFound(parent_id.into()))?;
            (parent_level, dest_children)
        }
        None => (0, &mut doc.headlines),
    };

    let delta = dest_parent_level.saturating_add(1) as i16 - node.level as i16;
    adjust_levels(&mut node, delta)?;

    let insert_index = match position {
        InsertPosition::Prepend => 0,
        InsertPosition::Append => dest_children.len(),
        InsertPosition::Index(idx) => idx.min(dest_children.len()),
    };

    dest_children.insert(insert_index, node);
    Ok(())
}

/// Insert a new headline beneath the given parent.
pub fn insert_headline(
    doc: &mut Document,
    parent_id: Option<&str>,
    position: InsertPosition,
    title: Vec<Object>,
) -> Result<NodePath, ToolError> {
    let (parent_level, children) = match parent_id {
        Some(id) => {
            let path =
                find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
            let parent = headline_mut_by_path(&mut doc.headlines, &path.0)
                .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
            (parent.level, &mut parent.children)
        }
        None => (0, &mut doc.headlines),
    };

    let level = parent_level.saturating_add(1);
    let headline = Headline {
        level,
        title,
        ..Headline::default()
    };

    let insert_index = match position {
        InsertPosition::Prepend => 0,
        InsertPosition::Append => children.len(),
        InsertPosition::Index(idx) => idx.min(children.len()),
    };

    children.insert(insert_index, headline);

    let mut path = Vec::new();
    if let Some(id) = parent_id {
        path = find_node_path_by_id(doc, id)
            .ok_or_else(|| ToolError::NodeNotFound(id.into()))?
            .0;
        path.push(insert_index);
    } else {
        path.push(insert_index);
    }

    Ok(NodePath(path))
}

/// Set a node's TODO keyword state.
pub fn set_state_by_id(
    doc: &mut Document,
    id: &str,
    keyword: Option<TodoKeyword>,
) -> Result<(), ToolError> {
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    headline.keyword = keyword;
    Ok(())
}

/// Set or clear a node's priority cookie.
pub fn set_priority_by_id(
    doc: &mut Document,
    id: &str,
    priority: Option<crate::Priority>,
) -> Result<(), ToolError> {
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    headline.priority = priority;
    Ok(())
}

/// Set the title of a node.
pub fn set_title_by_id(doc: &mut Document, id: &str, title: Vec<Object>) -> Result<(), ToolError> {
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    headline.title = title;
    Ok(())
}

/// Set or update a property in the node's property drawer.
pub fn set_property_by_id(
    doc: &mut Document,
    id: &str,
    name: &str,
    value: &str,
) -> Result<(), ToolError> {
    validate_property_name(name)?;
    validate_property_value(value)?;
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;

    let drawer = headline
        .properties
        .get_or_insert_with(PropertyDrawer::default);

    if let Some(existing) = drawer
        .properties
        .iter_mut()
        .find(|prop| prop.name().eq_ignore_ascii_case(name))
    {
        existing.value = value.to_string();
    } else {
        drawer.properties.push(NodeProperty::new(name, value)?);
    }

    Ok(())
}

/// Replace the tag list for a node.
pub fn set_tags_by_id(doc: &mut Document, id: &str, tags: &[&str]) -> Result<(), ToolError> {
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;

    let mut new_tags = Vec::new();
    for tag in tags {
        new_tags.push(Tag::new(*tag)?);
    }
    headline.tags = new_tags;
    Ok(())
}

/// Update a planning timestamp for a node.
pub fn set_planning_by_id(
    doc: &mut Document,
    id: &str,
    planning_type: PlanningType,
    timestamp: Option<Timestamp>,
) -> Result<(), ToolError> {
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;

    let planning = headline.planning.get_or_insert_with(Planning::default);
    match planning_type {
        PlanningType::Scheduled => planning.scheduled = timestamp,
        PlanningType::Deadline => planning.deadline = timestamp,
        PlanningType::Closed => planning.closed = timestamp,
    }

    if planning.deadline.is_none() && planning.scheduled.is_none() && planning.closed.is_none() {
        headline.planning = None;
    }

    Ok(())
}

/// Append a clock entry to a node's LOGBOOK drawer.
pub fn log_clock_by_id(doc: &mut Document, id: &str, clock: crate::Clock) -> Result<(), ToolError> {
    let path = find_node_path_by_id(doc, id).ok_or_else(|| ToolError::NodeNotFound(id.into()))?;
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(id.into()))?;

    let section = headline.section.get_or_insert_with(Section::default);

    for element in &mut section.elements {
        if let Element::Drawer(drawer) = element
            && drawer.name().eq_ignore_ascii_case("LOGBOOK")
        {
            drawer.contents_mut().push(Element::Clock(clock));
            return Ok(());
        }
    }

    let drawer = Drawer::new("LOGBOOK", vec![Element::Clock(clock)])?;
    section.elements.insert(0, Element::Drawer(drawer));
    Ok(())
}

/// Ensure a node has an ID, optionally preferring a specific value.
pub fn ensure_id(
    doc: &mut Document,
    path: &NodePath,
    preferred: Option<&str>,
) -> Result<String, ToolError> {
    let existing_headline = headline_by_path(&doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(format!("{:?}", path.0)))?;

    if let Some(existing) = headline_id(existing_headline) {
        return Ok(existing.to_string());
    }

    let mut ids = collect_ids(doc);
    let headline = headline_mut_by_path(&mut doc.headlines, &path.0)
        .ok_or_else(|| ToolError::NodeNotFound(format!("{:?}", path.0)))?;
    if let Some(pref) = preferred
        && !ids.contains(pref)
    {
        set_property_on_headline(headline, "ID", pref)?;
        return Ok(pref.to_string());
    }

    let new_id = generate_unique_id(&ids);
    ids.insert(new_id.clone());
    set_property_on_headline(headline, "ID", &new_id)?;
    Ok(new_id)
}

/// Build a clock entry from start/end timestamps, computing duration when possible.
pub fn clock_from_datetimes(start: DateTime, end: Option<DateTime>) -> crate::Clock {
    let timestamp = Timestamp {
        timestamp_type: if end.is_some() {
            TimestampType::InactiveRange
        } else {
            TimestampType::Inactive
        },
        start: start.clone(),
        end: end.clone(),
        repeater: None,
        warning: None,
    };

    let duration = if let Some(end_dt) = end {
        duration_between(&start, &end_dt)
    } else {
        None
    };

    crate::Clock {
        timestamp,
        duration,
    }
}

fn headline_id(headline: &Headline) -> Option<&str> {
    let props = headline.properties.as_ref()?;
    for prop in &props.properties {
        if prop.name().eq_ignore_ascii_case("ID") {
            return Some(prop.value());
        }
    }
    None
}

fn set_property_on_headline(
    headline: &mut Headline,
    name: &str,
    value: &str,
) -> Result<(), ToolError> {
    validate_property_name(name)?;
    validate_property_value(value)?;
    let drawer = headline
        .properties
        .get_or_insert_with(PropertyDrawer::default);
    if let Some(existing) = drawer
        .properties
        .iter_mut()
        .find(|prop| prop.name().eq_ignore_ascii_case(name))
    {
        existing.value = value.to_string();
    } else {
        drawer.properties.push(NodeProperty::new(name, value)?);
    }
    Ok(())
}

fn find_path_in_headlines(
    headlines: &[Headline],
    id: &str,
    prefix: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    for (idx, headline) in headlines.iter().enumerate() {
        if headline_id(headline) == Some(id) {
            let mut path = prefix.clone();
            path.push(idx);
            return Some(path);
        }
        prefix.push(idx);
        if let Some(found) = find_path_in_headlines(&headline.children, id, prefix) {
            return Some(found);
        }
        prefix.pop();
    }
    None
}

/// Get a mutable reference to a headline by path.
pub fn headline_mut_by_path<'a>(
    headlines: &'a mut [Headline],
    path: &[usize],
) -> Option<&'a mut Headline> {
    let (&idx, rest) = path.split_first()?;
    let headline = headlines.get_mut(idx)?;
    if rest.is_empty() {
        Some(headline)
    } else {
        headline_mut_by_path(&mut headline.children, rest)
    }
}

fn headline_by_path<'a>(headlines: &'a [Headline], path: &[usize]) -> Option<&'a Headline> {
    let (&idx, rest) = path.split_first()?;
    let headline = headlines.get(idx)?;
    if rest.is_empty() {
        Some(headline)
    } else {
        headline_by_path(&headline.children, rest)
    }
}

fn children_vec_mut<'a>(
    headlines: &'a mut Vec<Headline>,
    parent_path: Option<&NodePath>,
) -> Option<&'a mut Vec<Headline>> {
    match parent_path {
        Some(path) => headline_mut_by_path(headlines, &path.0).map(|h| &mut h.children),
        None => Some(headlines),
    }
}

fn parent_vec_and_level_mut<'a>(
    headlines: &'a mut Vec<Headline>,
    path: &[usize],
    parent_level: u8,
) -> Option<(&'a mut Vec<Headline>, usize, u8)> {
    let (&idx, rest) = path.split_first()?;
    if rest.is_empty() {
        return Some((headlines, idx, parent_level));
    }
    let headline = headlines.get_mut(idx)?;
    let level = headline.level;
    parent_vec_and_level_mut(&mut headline.children, rest, level)
}

fn adjust_levels(headline: &mut Headline, delta: i16) -> Result<(), ToolError> {
    let new_level = headline.level as i16 + delta;
    if new_level < 1 {
        return Err(ToolError::InvalidOperation(
            "headline level must be at least 1".to_string(),
        ));
    }
    headline.level = new_level as u8;
    for child in &mut headline.children {
        adjust_levels(child, delta)?;
    }
    Ok(())
}

fn path_is_prefix(prefix: &[usize], path: &[usize]) -> bool {
    if prefix.len() > path.len() {
        return false;
    }
    prefix.iter().zip(path.iter()).all(|(a, b)| a == b)
}

fn collect_ids(doc: &Document) -> HashSet<String> {
    let mut ids = HashSet::new();
    collect_ids_from_headlines(&doc.headlines, &mut ids);
    ids
}

fn collect_ids_from_headlines(headlines: &[Headline], ids: &mut HashSet<String>) {
    for headline in headlines {
        if let Some(id) = headline_id(headline) {
            ids.insert(id.to_string());
        }
        collect_ids_from_headlines(&headline.children, ids);
    }
}

fn generate_unique_id(existing: &HashSet<String>) -> String {
    loop {
        let candidate = Uuid::now_v7().to_string();
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
}

fn sanitize_body_content(content: &str) -> String {
    let mut out = String::new();
    for (idx, line) in content.lines().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        if is_headline_line(line) {
            out.push(',');
        }
        out.push_str(line);
    }

    if content.ends_with('\n') {
        out.push('\n');
    }

    out
}

fn is_headline_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    if !trimmed.starts_with('*') {
        return false;
    }
    let stars = trimmed.chars().take_while(|&c| c == '*').count();
    let after = &trimmed[stars..];
    after.is_empty() || after.starts_with(' ')
}

fn parse_section_from_body(content: &str) -> Result<Option<Section>, ToolError> {
    let mut wrapped = String::from("* _\n");
    wrapped.push_str(content);
    if !content.ends_with('\n') {
        wrapped.push('\n');
    }
    let doc = parse(&wrapped)?;
    Ok(doc.headlines.first().and_then(|h| h.section.clone()))
}

fn duration_between(start: &DateTime, end: &DateTime) -> Option<crate::Duration> {
    let start_minutes = datetime_to_minutes(start)?;
    let end_minutes = datetime_to_minutes(end)?;
    if end_minutes < start_minutes {
        return None;
    }
    let total_minutes = end_minutes - start_minutes;
    let hours = (total_minutes / 60) as u32;
    let minutes = (total_minutes % 60) as u8;
    Some(crate::Duration { hours, minutes })
}

fn datetime_to_minutes(dt: &DateTime) -> Option<i64> {
    let hour = dt.hour? as i64;
    let minute = dt.minute? as i64;
    let days = days_from_civil(dt.year, dt.month, dt.day)?;
    Some(days * 24 * 60 + hour * 60 + minute)
}

fn days_from_civil(year: i32, month: u8, day: u8) -> Option<i64> {
    let y = year as i64;
    let m = month as i64;
    let d = day as i64;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DateTime;
    use crate::Timestamp;
    use crate::TimestampType;

    fn doc_with_ids(input: &str) -> Document {
        parse(input).expect("parse test doc")
    }

    #[test]
    fn update_body_sanitizes_headlines() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: 123\n:END:\nOld body\n");
        update_body_by_id(
            &mut doc,
            "123",
            "* Injected\nParagraph",
            BodyUpdateMode::Replace,
        )
        .unwrap();
        let rendered = format!("{}", doc);
        assert!(rendered.contains(",* Injected"));
        assert!(rendered.contains("Paragraph"));
    }

    #[test]
    fn delete_node_promotes_children() {
        let mut doc = doc_with_ids(
            "* Parent\n:PROPERTIES:\n:ID: P\n:END:\n** Child\n:PROPERTIES:\n:ID: C\n:END:\n*** Grand\n:PROPERTIES:\n:ID: G\n:END:\n",
        );
        delete_node_by_id(&mut doc, "C", DeleteStrategy::PromoteChildren).unwrap();
        let rendered = format!("{}", doc);
        assert!(rendered.contains("** Grand"));
        assert!(!rendered.contains("** Child"));
    }

    #[test]
    fn refile_node_moves_headline() {
        let mut doc = doc_with_ids(
            "* A\n:PROPERTIES:\n:ID: A\n:END:\n* B\n:PROPERTIES:\n:ID: B\n:END:\n** X\n:PROPERTIES:\n:ID: X\n:END:\n",
        );
        refile_node_by_id(&mut doc, "X", Some("A"), InsertPosition::Append).unwrap();
        let rendered = format!("{}", doc);
        assert!(rendered.contains("* A"));
        assert!(rendered.contains("** X"));
    }

    #[test]
    fn log_clock_creates_logbook() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: 9\n:END:\n");
        let clock = clock_from_datetimes(
            DateTime {
                year: 2025,
                month: 1,
                day: 26,
                dayname: None,
                hour: Some(10),
                minute: Some(0),
                end_hour: None,
                end_minute: None,
            },
            Some(DateTime {
                year: 2025,
                month: 1,
                day: 26,
                dayname: None,
                hour: Some(11),
                minute: Some(0),
                end_hour: None,
                end_minute: None,
            }),
        );
        log_clock_by_id(&mut doc, "9", clock).unwrap();
        let rendered = format!("{}", doc);
        assert!(rendered.contains(":LOGBOOK:"));
        assert!(rendered.contains("CLOCK:"));
    }

    #[test]
    fn set_planning_updates_timestamp() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: 7\n:END:\n");
        let ts = Timestamp {
            timestamp_type: TimestampType::Active,
            start: DateTime {
                year: 2024,
                month: 2,
                day: 1,
                dayname: None,
                hour: None,
                minute: None,
                end_hour: None,
                end_minute: None,
            },
            end: None,
            repeater: None,
            warning: None,
        };
        set_planning_by_id(&mut doc, "7", PlanningType::Deadline, Some(ts)).unwrap();
        let rendered = format!("{}", doc);
        assert!(rendered.contains("DEADLINE:"));
    }

    #[test]
    fn ensure_id_assigns_preferred_when_free() {
        let mut doc = doc_with_ids("* Task\n");
        let path = NodePath(vec![0]);
        let id = ensure_id(&mut doc, &path, Some("ABC")).unwrap();
        assert_eq!(id, "ABC");
        let rendered = format!("{}", doc);
        assert!(rendered.contains(":ID: ABC"));
    }

    #[test]
    fn set_priority_by_id_sets_and_clears() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: pri1\n:END:\n");
        set_priority_by_id(&mut doc, "pri1", Some(crate::Priority::new('B').unwrap())).unwrap();
        let rendered = format!("{}", doc);
        println!("after set: {rendered}");
        assert!(rendered.contains("[#B]"));

        set_priority_by_id(&mut doc, "pri1", None).unwrap();
        let rendered = format!("{}", doc);
        println!("after clear: {rendered}");
        assert!(!rendered.contains("[#B]"));
    }

    #[test]
    fn set_priority_by_id_node_not_found() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: exists\n:END:\n");
        let err = set_priority_by_id(
            &mut doc,
            "missing",
            Some(crate::Priority::new('A').unwrap()),
        );
        println!("error: {err:?}");
        assert!(err.is_err());
    }

    #[test]
    fn set_title_by_id_renames() {
        let mut doc = doc_with_ids("* Old Title\n:PROPERTIES:\n:ID: t1\n:END:\n");
        let new_title = vec![crate::Object::Text(crate::Text {
            value: "New Title".to_string(),
        })];
        set_title_by_id(&mut doc, "t1", new_title).unwrap();
        let rendered = format!("{}", doc);
        println!("after rename: {rendered}");
        assert!(rendered.contains("New Title"));
        assert!(!rendered.contains("Old Title"));
    }

    #[test]
    fn set_title_by_id_node_not_found() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: exists\n:END:\n");
        let new_title = vec![crate::Object::Text(crate::Text {
            value: "Whatever".to_string(),
        })];
        let err = set_title_by_id(&mut doc, "missing", new_title);
        println!("error: {err:?}");
        assert!(err.is_err());
    }

    #[test]
    fn set_property_rejects_newline_in_value() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: pv1\n:END:\n");
        let err = set_property_by_id(&mut doc, "pv1", "CUSTOM", "line1\nline2");
        println!("newline error: {err:?}");
        assert!(err.is_err());
    }

    #[test]
    fn set_property_rejects_carriage_return_in_value() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: pv2\n:END:\n");
        let err = set_property_by_id(&mut doc, "pv2", "CUSTOM", "line1\rline2");
        println!("carriage return error: {err:?}");
        assert!(err.is_err());
    }

    #[test]
    fn set_property_rejects_nul_in_value() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: pv3\n:END:\n");
        let err = set_property_by_id(&mut doc, "pv3", "CUSTOM", "has\0nul");
        println!("nul error: {err:?}");
        assert!(err.is_err());
    }

    #[test]
    fn set_property_accepts_valid_value() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: pv4\n:END:\n");
        set_property_by_id(&mut doc, "pv4", "CUSTOM", "plain text value").unwrap();
        let rendered = format!("{}", doc);
        println!("valid property: {rendered}");
        assert!(rendered.contains(":CUSTOM: plain text value"));
    }

    #[test]
    fn update_existing_property_rejects_newline() {
        let mut doc = doc_with_ids("* Task\n:PROPERTIES:\n:ID: pv5\n:CUSTOM: old\n:END:\n");
        let err = set_property_by_id(&mut doc, "pv5", "CUSTOM", "new\nline");
        println!("update newline error: {err:?}");
        assert!(err.is_err());
    }

    #[test]
    fn set_property_by_id_preserves_other_fields() {
        let mut doc =
            doc_with_ids("* TODO [#A] Old Title :tag1:\n:PROPERTIES:\n:ID: t2\n:END:\nBody text\n");
        let new_title = vec![crate::Object::Text(crate::Text {
            value: "Renamed".to_string(),
        })];
        set_title_by_id(&mut doc, "t2", new_title).unwrap();
        let rendered = format!("{}", doc);
        println!("after rename preserving fields: {rendered}");
        assert!(rendered.contains("Renamed"));
        assert!(rendered.contains("TODO"));
        assert!(rendered.contains("[#A]"));
        assert!(rendered.contains(":tag1:"));
        assert!(rendered.contains("Body text"));
    }
}
