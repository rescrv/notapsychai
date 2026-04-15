use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;

use crate::api_types::{ConvergenceResponse, DelinquentItem, ScheduleItem};
use crate::{CadenceDocument, Error, Rhythm, RhythmID, Slider, StoredRhythm};

type Result<T> = std::result::Result<T, Error>;

fn invalid_input(message: impl Into<String>) -> Error {
    Error::Internal(message.into())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RhythmKind {
    Daily,
    Weekly,
    Monthly,
    EveryNDays,
}

impl RhythmKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::EveryNDays => "every-n-days",
        }
    }
}

impl From<Rhythm> for RhythmKind {
    fn from(rhythm: Rhythm) -> Self {
        match rhythm {
            Rhythm::Daily { .. } => Self::Daily,
            Rhythm::WeekDaily { .. } => Self::Weekly,
            Rhythm::Monthly { .. } => Self::Monthly,
            Rhythm::EveryNDays { .. } => Self::EveryNDays,
        }
    }
}

impl FromStr for RhythmKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_lowercase().as_str() {
            "daily" => Ok(Self::Daily),
            "weekly" | "weekdaily" => Ok(Self::Weekly),
            "monthly" => Ok(Self::Monthly),
            "every-n-days" | "every_n_days" => Ok(Self::EveryNDays),
            _ => Err(invalid_input(format!(
                "Invalid rhythm type: '{value}'. Must be daily, weekly, monthly, or every-n-days"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RhythmInput {
    pub kind: Option<RhythmKind>,
    pub at: Option<NaiveTime>,
    pub dotw: Option<u8>,
    pub dotm: Option<u32>,
    pub every_n_days: Option<u32>,
    pub slider_before: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RhythmUpdate {
    pub rhythm: Option<Rhythm>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TodayRenderOptions {
    pub include_regular_heading: bool,
}

pub fn default_file_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rhythm")
        .join("cadence.yaml")
}

pub fn parse_time(time_str: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(time_str, "%H:%M:%S").map_err(|e| {
        invalid_input(format!(
            "Failed to parse time '{time_str}': {e}. Expected format: HH:MM:SS (e.g., 09:30:00)"
        ))
    })
}

pub fn parse_date(date_str: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|e| {
        invalid_input(format!(
            "Failed to parse date '{date_str}': {e}. Expected format: YYYY-MM-DD"
        ))
    })
}

pub fn parse_rhythm_id(id: &str) -> Result<RhythmID> {
    RhythmID::from_human_readable(id)
        .ok_or_else(|| invalid_input(format!("Invalid rhythm ID: {id}")))
}

pub fn document_timezone(document: &CadenceDocument) -> Result<Tz> {
    document
        .timezone
        .parse()
        .map_err(|e: chrono_tz::ParseError| invalid_input(e.to_string()))
}

pub fn load_document(path: &Path) -> Result<CadenceDocument> {
    CadenceDocument::load_or_default(path)
}

pub fn save_document(path: &Path, document: &CadenceDocument) -> Result<()> {
    document.save(path)
}

pub fn rhythm_kind_name(rhythm: Rhythm) -> &'static str {
    RhythmKind::from(rhythm).as_str()
}

pub fn list_rhythms<'a>(
    document: &'a CadenceDocument,
    pattern: Option<&str>,
    kind: Option<RhythmKind>,
) -> Vec<&'a StoredRhythm> {
    let pattern = pattern.map(str::to_lowercase);

    document
        .rhythms
        .iter()
        .filter(|rhythm| {
            pattern
                .as_ref()
                .map(|pattern| rhythm.description.to_lowercase().contains(pattern))
                .unwrap_or(true)
        })
        .filter(|rhythm| {
            kind.map(|kind| RhythmKind::from(rhythm.rhythm) == kind)
                .unwrap_or(true)
        })
        .collect()
}

pub fn format_rhythm(rhythm: &StoredRhythm) -> String {
    let mut out = String::new();
    out.push_str(&format!("ID: {}\n", rhythm.id));
    out.push_str(&format!("Description: {}\n", rhythm.description));
    out.push_str(&format!("Type: {}\n", rhythm_kind_name(rhythm.rhythm)));
    out.push_str(&format!("Schedule: {}\n", rhythm.rhythm));
    out.push_str(&format!(
        "Created: {} ({})\n",
        rhythm.created_at, rhythm.created_at_tz
    ));
    out.push_str(&format!(
        "Modified: {} ({})\n",
        rhythm.modified_at, rhythm.modified_at_tz
    ));
    if let Some(last_done) = &rhythm.last_done {
        out.push_str(&format!(
            "Last done: {} ({})\n",
            last_done.when, last_done.when_tz
        ));
    }
    if let Some(last_defer) = &rhythm.last_defer {
        out.push_str(&format!(
            "Last defer: {} ({})\n",
            last_defer.when, last_defer.when_tz
        ));
    }
    out
}

pub fn build_rhythm(input: RhythmInput, current: Option<Rhythm>) -> Result<Rhythm> {
    let kind = input
        .kind
        .or_else(|| current.map(RhythmKind::from))
        .ok_or_else(|| invalid_input("Rhythm type is required"))?;
    let at = input
        .at
        .or_else(|| current.map(Rhythm::at))
        .ok_or_else(|| invalid_input("Time is required"))?;
    let slider = input
        .slider_before
        .map(Slider::new)
        .or_else(|| current.map(Rhythm::slider))
        .unwrap_or_default();

    match kind {
        RhythmKind::Daily => Ok(Rhythm::Daily { at }),
        RhythmKind::Weekly => {
            let dotw = input
                .dotw
                .or(match current {
                    Some(Rhythm::WeekDaily { dotw, .. }) => Some(dotw),
                    _ => None,
                })
                .ok_or_else(|| invalid_input("Day of week is required for weekly rhythms"))?;
            if dotw > 6 {
                return Err(invalid_input(
                    "Day of week must be in range 0-6 (0=Monday, 6=Sunday)",
                ));
            }
            Ok(Rhythm::WeekDaily { dotw, at, slider })
        }
        RhythmKind::Monthly => {
            let dotm = input
                .dotm
                .or(match current {
                    Some(Rhythm::Monthly { dotm, .. }) => Some(dotm),
                    _ => None,
                })
                .ok_or_else(|| invalid_input("Day of month is required for monthly rhythms"))?;
            if dotm > 30 {
                return Err(invalid_input("Day of month must be in range 0-30"));
            }
            Ok(Rhythm::Monthly { dotm, at, slider })
        }
        RhythmKind::EveryNDays => {
            let n = input
                .every_n_days
                .or(match current {
                    Some(Rhythm::EveryNDays { n, .. }) => Some(n),
                    _ => None,
                })
                .ok_or_else(|| {
                    invalid_input("Number of days is required for every-n-days rhythms")
                })?;
            if n == 0 {
                return Err(invalid_input("Number of days must be greater than 0"));
            }
            Ok(Rhythm::EveryNDays { n, at, slider })
        }
    }
}

pub fn add_rhythm(
    document: &mut CadenceDocument,
    rhythm: Rhythm,
    description: String,
    now: DateTime<Utc>,
) -> Result<RhythmID> {
    let id = RhythmID::generate().ok_or_else(|| invalid_input("Failed to generate rhythm ID"))?;
    let timezone = document.timezone.clone();
    document.rhythms.push(StoredRhythm {
        id,
        rhythm,
        description,
        created_at: now,
        created_at_tz: timezone.clone(),
        modified_at: now,
        modified_at_tz: timezone,
        last_done: None,
        last_defer: None,
    });
    Ok(id)
}

pub fn update_rhythm(
    document: &mut CadenceDocument,
    rhythm_id: RhythmID,
    rhythm_update: RhythmUpdate,
    description: Option<String>,
) -> Result<()> {
    let timezone = document.timezone.clone();
    let stored = document
        .find_rhythm_mut(rhythm_id)
        .ok_or_else(|| invalid_input(format!("Rhythm not found: {rhythm_id}")))?;

    if let Some(rhythm) = rhythm_update.rhythm {
        stored.rhythm = rhythm;
    }
    if let Some(description) = description {
        stored.description = description;
    }
    stored.modified_at = rhythm_update.modified_at;
    stored.modified_at_tz = timezone;

    Ok(())
}

pub fn delete_rhythm(document: &mut CadenceDocument, rhythm_id: RhythmID) -> Result<()> {
    let original_len = document.rhythms.len();
    document.rhythms.retain(|rhythm| rhythm.id != rhythm_id);
    if document.rhythms.len() == original_len {
        return Err(invalid_input(format!("Rhythm not found: {rhythm_id}")));
    }
    Ok(())
}

pub fn today_items(document: &CadenceDocument) -> Result<Vec<ScheduleItem>> {
    let manager = document.to_manager()?;
    let today = manager.now().date_naive();
    let limit = today
        .checked_add_days(chrono::Days::new(90))
        .ok_or_else(|| invalid_input("Date overflow"))?;

    let schedule_without_adjustment = manager.schedule2(today, limit, false);
    let schedule_with_adjustment = manager.schedule2(today, limit, true);

    let with_adjustment_ids: HashSet<_> = schedule_with_adjustment
        .iter()
        .filter(|(dt, _)| dt.date_naive() == today)
        .map(|(_, rhythm_def)| rhythm_def.id)
        .collect();

    Ok(schedule_without_adjustment
        .into_iter()
        .filter(|(dt, _)| dt.date_naive() == today)
        .map(|(datetime, rhythm_def)| ScheduleItem {
            rhythm_id: rhythm_def.id.to_string(),
            description: rhythm_def.description,
            datetime: datetime.with_timezone(&Utc),
            stretch_goal: !with_adjustment_ids.contains(&rhythm_def.id),
        })
        .collect())
}

pub fn render_today_items(
    items: &[ScheduleItem],
    user_tz: &Tz,
    options: TodayRenderOptions,
) -> String {
    if items.is_empty() {
        return "No scheduled items for today.".to_string();
    }

    let regular_items: Vec<_> = items.iter().filter(|item| !item.stretch_goal).collect();
    let stretch_items: Vec<_> = items.iter().filter(|item| item.stretch_goal).collect();
    let mut lines = Vec::new();

    if !regular_items.is_empty() {
        if options.include_regular_heading {
            lines.push("Today's items:".to_string());
        }
        for item in regular_items {
            let local_time = item.datetime.with_timezone(user_tz);
            lines.push(format!(
                "{} - {} [{}]",
                local_time.format("%H:%M:%S"),
                item.description,
                item.rhythm_id
            ));
        }
    }

    if !stretch_items.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("Stretch goals:".to_string());
        for item in stretch_items {
            let local_time = item.datetime.with_timezone(user_tz);
            lines.push(format!(
                "{} - {} [{}]",
                local_time.format("%H:%M:%S"),
                item.description,
                item.rhythm_id
            ));
        }
    }

    lines.join("\n")
}

pub fn schedule_items(
    document: &CadenceDocument,
    start_date: NaiveDate,
    days: u32,
) -> Result<Vec<ScheduleItem>> {
    let manager = document.to_manager()?;
    let limit = start_date
        .checked_add_days(chrono::Days::new(days as u64))
        .ok_or_else(|| invalid_input("Invalid date range"))?;

    Ok(manager
        .schedule(start_date, limit)
        .into_iter()
        .map(|(datetime, rhythm_def)| ScheduleItem {
            rhythm_id: rhythm_def.id.to_string(),
            description: rhythm_def.description,
            datetime: datetime.with_timezone(&Utc),
            stretch_goal: false,
        })
        .collect())
}

pub fn render_schedule_items(items: &[ScheduleItem], user_tz: &Tz, empty_message: &str) -> String {
    if items.is_empty() {
        return empty_message.to_string();
    }

    items
        .iter()
        .map(|item| {
            let local_time = item.datetime.with_timezone(user_tz);
            format!(
                "{} - {} [{}]",
                local_time.format("%Y-%m-%d %H:%M:%S"),
                item.description,
                item.rhythm_id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn delinquent_items(document: &CadenceDocument) -> Result<Vec<DelinquentItem>> {
    let manager = document.to_manager()?;
    Ok(manager
        .delinquent()
        .into_iter()
        .map(|rhythm_def| DelinquentItem {
            rhythm_id: rhythm_def.id.to_string(),
            description: rhythm_def.description,
        })
        .collect())
}

pub fn render_delinquent_items(items: &[DelinquentItem]) -> String {
    if items.is_empty() {
        return "No delinquent rhythms.".to_string();
    }

    items
        .iter()
        .map(|item| format!("{} [{}]", item.description, item.rhythm_id))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn convergence_response(document: &CadenceDocument) -> Result<ConvergenceResponse> {
    let manager = document.to_manager()?;
    let datetime = if manager.rhythms().is_empty() {
        None
    } else {
        Some(manager.convergence())
    };
    Ok(ConvergenceResponse { datetime })
}

fn apply_manager_update<F>(document: &mut CadenceDocument, update: F) -> Result<()>
where
    F: FnOnce(&mut crate::RhythmManager) -> Result<()>,
{
    let mut manager = document.to_manager()?;
    update(&mut manager)?;
    document.update_from_manager(&manager);
    Ok(())
}

pub fn mark_rhythm_done(document: &mut CadenceDocument, rhythm_id: RhythmID) -> Result<()> {
    mark_rhythm_done_at(document, rhythm_id, Utc::now())
}

pub fn mark_rhythm_done_at(
    document: &mut CadenceDocument,
    rhythm_id: RhythmID,
    when: DateTime<Utc>,
) -> Result<()> {
    apply_manager_update(document, |manager| manager.mark_done_at(rhythm_id, when))
}

pub fn defer_rhythm(document: &mut CadenceDocument, rhythm_id: RhythmID) -> Result<()> {
    defer_rhythm_at(document, rhythm_id, Utc::now())
}

pub fn defer_rhythm_at(
    document: &mut CadenceDocument,
    rhythm_id: RhythmID,
    when: DateTime<Utc>,
) -> Result<()> {
    apply_manager_update(document, |manager| manager.defer_rhythm_at(rhythm_id, when))
}

pub fn set_spoons(document: &mut CadenceDocument, date: NaiveDate, value: u8) -> Result<()> {
    if value > 10 {
        return Err(invalid_input("Spoons value must be in range 0-10"));
    }
    apply_manager_update(document, |manager| {
        manager.spoons(date, value);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::StoredEventRecord;

    fn time(hour: u32, minute: u32, second: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hour, minute, second).unwrap()
    }

    fn timestamp(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, day, hour, minute, 0)
            .single()
            .unwrap()
    }

    fn weekly_rhythm(dotw: u8, at: NaiveTime, slider_before: u32) -> Rhythm {
        Rhythm::WeekDaily {
            dotw,
            at,
            slider: Slider::new(slider_before),
        }
    }

    fn sample_document(rhythm_id: RhythmID) -> CadenceDocument {
        CadenceDocument {
            version: crate::store::CURRENT_DOCUMENT_VERSION,
            timezone: "UTC".to_string(),
            spoons: Default::default(),
            rhythms: vec![StoredRhythm {
                id: rhythm_id,
                rhythm: weekly_rhythm(2, time(9, 0, 0), 1),
                description: "Morning meds".to_string(),
                created_at: timestamp(1, 9, 0),
                created_at_tz: "UTC".to_string(),
                modified_at: timestamp(1, 9, 0),
                modified_at_tz: "UTC".to_string(),
                last_done: None,
                last_defer: None,
            }],
        }
    }

    #[test]
    fn rhythm_kind_parses_aliases() {
        assert_eq!(RhythmKind::EveryNDays, "every_n_days".parse().unwrap());
        assert_eq!(RhythmKind::EveryNDays, "every-n-days".parse().unwrap());
        assert_eq!(RhythmKind::Weekly, "weekly".parse().unwrap());
        assert_eq!(RhythmKind::Weekly, "weekdaily".parse().unwrap());
    }

    #[test]
    fn build_rhythm_reuses_current_values() {
        let rhythm = build_rhythm(
            RhythmInput {
                at: Some(time(10, 30, 0)),
                slider_before: Some(3),
                ..RhythmInput::default()
            },
            Some(weekly_rhythm(2, time(9, 0, 0), 1)),
        )
        .unwrap();

        assert_eq!(
            Rhythm::WeekDaily {
                dotw: 2,
                at: time(10, 30, 0),
                slider: Slider::new(3),
            },
            rhythm
        );
    }

    #[test]
    fn build_rhythm_requires_new_kind_specific_fields() {
        let err = build_rhythm(
            RhythmInput {
                kind: Some(RhythmKind::Monthly),
                ..RhythmInput::default()
            },
            Some(Rhythm::Daily { at: time(9, 0, 0) }),
        )
        .unwrap_err();

        assert_eq!(
            "Day of month is required for monthly rhythms",
            err.to_string()
        );
    }

    #[test]
    fn add_rhythm_populates_metadata() {
        let now = timestamp(3, 8, 15);
        let mut document = CadenceDocument::new("UTC");
        let rhythm = Rhythm::Daily { at: time(8, 0, 0) };
        let rhythm_id =
            add_rhythm(&mut document, rhythm, "Take medication".to_string(), now).unwrap();

        assert_eq!(
            vec![StoredRhythm {
                id: rhythm_id,
                rhythm: Rhythm::Daily { at: time(8, 0, 0) },
                description: "Take medication".to_string(),
                created_at: now,
                created_at_tz: "UTC".to_string(),
                modified_at: now,
                modified_at_tz: "UTC".to_string(),
                last_done: None,
                last_defer: None,
            }],
            document.rhythms
        );
    }

    #[test]
    fn update_rhythm_changes_requested_fields_only() {
        let rhythm_id = RhythmID::generate().unwrap();
        let mut document = sample_document(rhythm_id);

        update_rhythm(
            &mut document,
            rhythm_id,
            RhythmUpdate {
                rhythm: Some(weekly_rhythm(4, time(7, 30, 0), 2)),
                modified_at: timestamp(4, 7, 30),
            },
            Some("Physical therapy".to_string()),
        )
        .unwrap();

        assert_eq!(
            vec![StoredRhythm {
                id: rhythm_id,
                rhythm: weekly_rhythm(4, time(7, 30, 0), 2),
                description: "Physical therapy".to_string(),
                created_at: timestamp(1, 9, 0),
                created_at_tz: "UTC".to_string(),
                modified_at: timestamp(4, 7, 30),
                modified_at_tz: "UTC".to_string(),
                last_done: None,
                last_defer: None,
            }],
            document.rhythms
        );
    }

    #[test]
    fn delete_rhythm_removes_matching_entry() {
        let rhythm_id = RhythmID::generate().unwrap();
        let mut document = sample_document(rhythm_id);

        delete_rhythm(&mut document, rhythm_id).unwrap();

        assert_eq!(Vec::<StoredRhythm>::new(), document.rhythms);
    }

    #[test]
    fn mark_rhythm_done_at_records_latest_done() {
        let rhythm_id = RhythmID::generate().unwrap();
        let mut document = sample_document(rhythm_id);
        let when = timestamp(5, 12, 0);

        mark_rhythm_done_at(&mut document, rhythm_id, when).unwrap();

        assert_eq!(
            vec![StoredRhythm {
                id: rhythm_id,
                rhythm: weekly_rhythm(2, time(9, 0, 0), 1),
                description: "Morning meds".to_string(),
                created_at: timestamp(1, 9, 0),
                created_at_tz: "UTC".to_string(),
                modified_at: timestamp(1, 9, 0),
                modified_at_tz: "UTC".to_string(),
                last_done: Some(StoredEventRecord {
                    when,
                    when_tz: "UTC".to_string(),
                }),
                last_defer: None,
            }],
            document.rhythms
        );
    }

    #[test]
    fn defer_rhythm_at_records_latest_defer() {
        let rhythm_id = RhythmID::generate().unwrap();
        let mut document = sample_document(rhythm_id);
        let when = timestamp(6, 13, 15);

        defer_rhythm_at(&mut document, rhythm_id, when).unwrap();

        assert_eq!(
            vec![StoredRhythm {
                id: rhythm_id,
                rhythm: weekly_rhythm(2, time(9, 0, 0), 1),
                description: "Morning meds".to_string(),
                created_at: timestamp(1, 9, 0),
                created_at_tz: "UTC".to_string(),
                modified_at: timestamp(1, 9, 0),
                modified_at_tz: "UTC".to_string(),
                last_done: None,
                last_defer: Some(StoredEventRecord {
                    when,
                    when_tz: "UTC".to_string(),
                }),
            }],
            document.rhythms
        );
    }

    #[test]
    fn set_spoons_rejects_values_out_of_range() {
        let mut document = CadenceDocument::new("UTC");
        let err = set_spoons(
            &mut document,
            NaiveDate::from_ymd_opt(2026, 4, 9).unwrap(),
            11,
        )
        .unwrap_err();

        assert_eq!("Spoons value must be in range 0-10", err.to_string());
    }
}
