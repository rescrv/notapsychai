//! Validation functions for Org-mode constructs.

/// Error returned when validation fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    /// Description of what was invalid.
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ValidationError {}

impl ValidationError {
    /// Create a new validation error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Validate that a string is a valid TODO keyword.
///
/// TODO keywords must:
/// - Be non-empty
/// - Contain only uppercase ASCII letters
pub fn validate_todo_keyword(keyword: &str) -> Result<(), ValidationError> {
    if keyword.is_empty() {
        return Err(ValidationError::new("TODO keyword cannot be empty"));
    }
    if !keyword.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(ValidationError::new(format!(
            "TODO keyword '{}' must contain only uppercase ASCII letters",
            keyword
        )));
    }
    Ok(())
}

/// Validate that a character is a valid priority.
///
/// Priorities must be a single uppercase ASCII letter (A-Z).
pub fn validate_priority(value: char) -> Result<(), ValidationError> {
    if !value.is_ascii_uppercase() {
        return Err(ValidationError::new(format!(
            "priority '{}' must be an uppercase ASCII letter (A-Z)",
            value
        )));
    }
    Ok(())
}

/// Validate that a string is a valid tag.
///
/// Tags must:
/// - Be non-empty
/// - Contain only alphanumeric characters, underscores, and @ symbols
/// - Not contain colons (those are delimiters)
pub fn validate_tag(tag: &str) -> Result<(), ValidationError> {
    if tag.is_empty() {
        return Err(ValidationError::new("tag cannot be empty"));
    }
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@')
    {
        return Err(ValidationError::new(format!(
            "tag '{}' must contain only alphanumeric characters, underscores, and @",
            tag
        )));
    }
    Ok(())
}

/// Validate that a string is a valid property name.
///
/// Property names must:
/// - Be non-empty
/// - Contain only alphanumeric characters, underscores, and hyphens
/// - Not be "END" (reserved)
pub fn validate_property_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::new("property name cannot be empty"));
    }
    if name.eq_ignore_ascii_case("END") {
        return Err(ValidationError::new(
            "property name cannot be 'END' (reserved)",
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ValidationError::new(format!(
            "property name '{}' must contain only alphanumeric characters, underscores, and hyphens",
            name
        )));
    }
    Ok(())
}

/// Validate that a property value is safe for a single property drawer line.
///
/// Property values must not contain line breaks or NUL bytes.
pub fn validate_property_value(value: &str) -> Result<(), ValidationError> {
    if value.contains('\n') || value.contains('\r') {
        return Err(ValidationError::new(
            "property value cannot contain line breaks",
        ));
    }
    if value.contains('\0') {
        return Err(ValidationError::new(
            "property value cannot contain NUL bytes",
        ));
    }
    Ok(())
}

/// Validate that a string is a valid drawer name.
///
/// Drawer names must:
/// - Be non-empty
/// - Contain only alphanumeric characters, underscores, and hyphens
/// - Not be "END" or "PROPERTIES" (reserved)
pub fn validate_drawer_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::new("drawer name cannot be empty"));
    }
    if name.eq_ignore_ascii_case("END") || name.eq_ignore_ascii_case("PROPERTIES") {
        return Err(ValidationError::new(format!(
            "drawer name cannot be '{}' (reserved)",
            name
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ValidationError::new(format!(
            "drawer name '{}' must contain only alphanumeric characters, underscores, and hyphens",
            name
        )));
    }
    Ok(())
}

/// Validate a headline level.
///
/// Levels must be at least 1.
pub fn validate_headline_level(level: u8) -> Result<(), ValidationError> {
    if level == 0 {
        return Err(ValidationError::new("headline level must be at least 1"));
    }
    Ok(())
}

/// Validate a month value.
///
/// Months must be 1-12.
pub fn validate_month(month: u8) -> Result<(), ValidationError> {
    if !(1..=12).contains(&month) {
        return Err(ValidationError::new(format!(
            "month {} must be between 1 and 12",
            month
        )));
    }
    Ok(())
}

/// Validate a day value.
///
/// Days must be 1-31.
pub fn validate_day(day: u8) -> Result<(), ValidationError> {
    if !(1..=31).contains(&day) {
        return Err(ValidationError::new(format!(
            "day {} must be between 1 and 31",
            day
        )));
    }
    Ok(())
}

/// Validate an hour value.
///
/// Hours must be 0-23.
pub fn validate_hour(hour: u8) -> Result<(), ValidationError> {
    if hour > 23 {
        return Err(ValidationError::new(format!(
            "hour {} must be between 0 and 23",
            hour
        )));
    }
    Ok(())
}

/// Validate a minute value.
///
/// Minutes must be 0-59.
pub fn validate_minute(minute: u8) -> Result<(), ValidationError> {
    if minute > 59 {
        return Err(ValidationError::new(format!(
            "minute {} must be between 0 and 59",
            minute
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_todo_keywords() {
        assert!(validate_todo_keyword("TODO").is_ok());
        assert!(validate_todo_keyword("DONE").is_ok());
        assert!(validate_todo_keyword("WAITING").is_ok());
        assert!(validate_todo_keyword("CANCELLED").is_ok());
        assert!(validate_todo_keyword("A").is_ok());
    }

    #[test]
    fn invalid_todo_keywords() {
        assert!(validate_todo_keyword("").is_err());
        assert!(validate_todo_keyword("todo").is_err());
        assert!(validate_todo_keyword("Todo").is_err());
        assert!(validate_todo_keyword("TO DO").is_err());
        assert!(validate_todo_keyword("TODO!").is_err());
        assert!(validate_todo_keyword("123").is_err());
    }

    #[test]
    fn valid_priorities() {
        assert!(validate_priority('A').is_ok());
        assert!(validate_priority('B').is_ok());
        assert!(validate_priority('C').is_ok());
        assert!(validate_priority('Z').is_ok());
    }

    #[test]
    fn invalid_priorities() {
        assert!(validate_priority('a').is_err());
        assert!(validate_priority('1').is_err());
        assert!(validate_priority(' ').is_err());
        assert!(validate_priority('#').is_err());
    }

    #[test]
    fn valid_tags() {
        assert!(validate_tag("work").is_ok());
        assert!(validate_tag("home").is_ok());
        assert!(validate_tag("project_alpha").is_ok());
        assert!(validate_tag("@computer").is_ok());
        assert!(validate_tag("v2").is_ok());
        assert!(validate_tag("URGENT").is_ok());
    }

    #[test]
    fn invalid_tags() {
        assert!(validate_tag("").is_err());
        assert!(validate_tag("tag:with:colons").is_err());
        assert!(validate_tag("tag with spaces").is_err());
        assert!(validate_tag("tag-with-dash").is_err());
    }

    #[test]
    fn valid_property_names() {
        assert!(validate_property_name("ID").is_ok());
        assert!(validate_property_name("CUSTOM_ID").is_ok());
        assert!(validate_property_name("my-property").is_ok());
        assert!(validate_property_name("Property123").is_ok());
    }

    #[test]
    fn invalid_property_names() {
        assert!(validate_property_name("").is_err());
        assert!(validate_property_name("END").is_err());
        assert!(validate_property_name("end").is_err());
        assert!(validate_property_name("prop:value").is_err());
        assert!(validate_property_name("prop value").is_err());
    }

    #[test]
    fn valid_property_values() {
        assert!(validate_property_value("").is_ok());
        assert!(validate_property_value("abc 123").is_ok());
        assert!(validate_property_value("tab\tok").is_ok());
    }

    #[test]
    fn invalid_property_values() {
        assert!(validate_property_value("line1\nline2").is_err());
        assert!(validate_property_value("line1\rline2").is_err());
        assert!(validate_property_value("nul\0byte").is_err());
    }

    #[test]
    fn valid_drawer_names() {
        assert!(validate_drawer_name("LOGBOOK").is_ok());
        assert!(validate_drawer_name("NOTES").is_ok());
        assert!(validate_drawer_name("my_drawer").is_ok());
        assert!(validate_drawer_name("drawer-1").is_ok());
    }

    #[test]
    fn invalid_drawer_names() {
        assert!(validate_drawer_name("").is_err());
        assert!(validate_drawer_name("END").is_err());
        assert!(validate_drawer_name("PROPERTIES").is_err());
        assert!(validate_drawer_name("properties").is_err());
        assert!(validate_drawer_name("drawer:name").is_err());
    }

    #[test]
    fn valid_headline_levels() {
        assert!(validate_headline_level(1).is_ok());
        assert!(validate_headline_level(10).is_ok());
        assert!(validate_headline_level(255).is_ok());
    }

    #[test]
    fn invalid_headline_levels() {
        assert!(validate_headline_level(0).is_err());
    }

    #[test]
    fn valid_dates() {
        assert!(validate_month(1).is_ok());
        assert!(validate_month(12).is_ok());
        assert!(validate_day(1).is_ok());
        assert!(validate_day(31).is_ok());
        assert!(validate_hour(0).is_ok());
        assert!(validate_hour(23).is_ok());
        assert!(validate_minute(0).is_ok());
        assert!(validate_minute(59).is_ok());
    }

    #[test]
    fn invalid_dates() {
        assert!(validate_month(0).is_err());
        assert!(validate_month(13).is_err());
        assert!(validate_day(0).is_err());
        assert!(validate_day(32).is_err());
        assert!(validate_hour(24).is_err());
        assert!(validate_minute(60).is_err());
    }
}
