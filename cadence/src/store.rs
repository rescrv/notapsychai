use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use uuid::Uuid;

use crate::{Error, EventRecord, EventType, RhythmDefinition, RhythmID, RhythmManager};

pub const CURRENT_DOCUMENT_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct StoredEventRecord {
    pub when: DateTime<Utc>,
    pub when_tz: String,
}

impl StoredEventRecord {
    fn from_event(event: &EventRecord) -> Self {
        Self {
            when: event.when,
            when_tz: event.when_tz.clone(),
        }
    }

    fn to_event(&self, rhythm_id: RhythmID, event_type: EventType) -> EventRecord {
        EventRecord {
            rhythm_id,
            event_type,
            when: self.when,
            when_tz: self.when_tz.clone(),
        }
    }

    fn validate(&self) -> Result<(), Error> {
        Tz::from_str(&self.when_tz).map_err(|err| Error::Internal(err.to_string()))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct StoredRhythm {
    pub id: RhythmID,
    pub rhythm: crate::Rhythm,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub created_at_tz: String,
    pub modified_at: DateTime<Utc>,
    pub modified_at_tz: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_done: Option<StoredEventRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_defer: Option<StoredEventRecord>,
}

impl StoredRhythm {
    pub fn to_definition(&self) -> RhythmDefinition {
        RhythmDefinition {
            id: self.id,
            rhythm: self.rhythm,
            description: self.description.clone(),
            created_at: self.created_at,
            created_at_tz: self.created_at_tz.clone(),
            modified_at: self.modified_at,
            modified_at_tz: self.modified_at_tz.clone(),
        }
    }

    fn from_definition(
        definition: RhythmDefinition,
        last_done: Option<&EventRecord>,
        last_defer: Option<&EventRecord>,
    ) -> Self {
        Self {
            id: definition.id,
            rhythm: definition.rhythm,
            description: definition.description,
            created_at: definition.created_at,
            created_at_tz: definition.created_at_tz,
            modified_at: definition.modified_at,
            modified_at_tz: definition.modified_at_tz,
            last_done: last_done.map(StoredEventRecord::from_event),
            last_defer: last_defer.map(StoredEventRecord::from_event),
        }
    }

    fn validate(&self) -> Result<(), Error> {
        Tz::from_str(&self.created_at_tz).map_err(|err| Error::Internal(err.to_string()))?;
        Tz::from_str(&self.modified_at_tz).map_err(|err| Error::Internal(err.to_string()))?;
        if let Some(last_done) = &self.last_done {
            last_done.validate()?;
        }
        if let Some(last_defer) = &self.last_defer {
            last_defer.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CadenceDocument {
    pub version: u32,
    pub timezone: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub spoons: BTreeMap<NaiveDate, u8>,
    #[serde(default)]
    pub rhythms: Vec<StoredRhythm>,
}

impl CadenceDocument {
    pub fn new(timezone: impl Into<String>) -> Self {
        Self {
            version: CURRENT_DOCUMENT_VERSION,
            timezone: timezone.into(),
            spoons: BTreeMap::new(),
            rhythms: Vec::new(),
        }
    }

    pub fn load_or_default(path: &Path) -> Result<Self, Error> {
        if path.exists() {
            let contents = fs::read_to_string(path)?;
            let document: Self = serde_yaml::from_str(&contents)?;
            document.validate()?;
            Ok(document)
        } else {
            Ok(Self::new(default_timezone_string()))
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        self.validate()?;

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let mut yaml = serde_yaml::to_string(self)?;
        if !yaml.ends_with('\n') {
            yaml.push('\n');
        }

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("cadence.yaml");
        let tmp_path = parent.join(format!(
            ".{file_name}.tmp-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }

        let mut file = options.open(&tmp_path)?;
        file.write_all(yaml.as_bytes())?;
        file.sync_all()?;

        #[cfg(unix)]
        {
            fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
        }

        fs::rename(&tmp_path, path)?;

        #[cfg(unix)]
        {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    pub fn to_manager(&self) -> Result<RhythmManager, Error> {
        self.validate()?;

        let rhythms = self
            .rhythms
            .iter()
            .map(StoredRhythm::to_definition)
            .collect::<Vec<_>>();
        let mut events = Vec::new();
        for rhythm in &self.rhythms {
            if let Some(last_done) = &rhythm.last_done {
                events.push(last_done.to_event(rhythm.id, EventType::Done));
            }
            if let Some(last_defer) = &rhythm.last_defer {
                events.push(last_defer.to_event(rhythm.id, EventType::Defer));
            }
        }
        events.sort_by_key(|event| event.when);

        let spoons = self
            .spoons
            .iter()
            .map(|(date, value)| (*date, (*value).min(10)))
            .collect::<HashMap<_, _>>();

        RhythmManager::new(Uuid::nil(), &self.timezone, rhythms, events, spoons)
    }

    pub fn update_from_manager(&mut self, manager: &RhythmManager) {
        self.version = CURRENT_DOCUMENT_VERSION;
        self.timezone = manager.timezone().to_string();
        self.spoons = manager
            .show_spoons()
            .into_iter()
            .map(|(date, value)| (date, value.min(10)))
            .collect();

        let mut last_done: HashMap<RhythmID, &EventRecord> = HashMap::new();
        let mut last_defer: HashMap<RhythmID, &EventRecord> = HashMap::new();
        for event in manager.events() {
            let target = match event.event_type {
                EventType::Done => &mut last_done,
                EventType::Defer => &mut last_defer,
            };
            let replace = target
                .get(&event.rhythm_id)
                .map(|current| current.when < event.when)
                .unwrap_or(true);
            if replace {
                target.insert(event.rhythm_id, event);
            }
        }

        self.rhythms = manager
            .rhythms()
            .into_iter()
            .map(|definition| {
                let id = definition.id;
                StoredRhythm::from_definition(
                    definition,
                    last_done.get(&id).copied(),
                    last_defer.get(&id).copied(),
                )
            })
            .collect();
    }

    pub fn find_rhythm(&self, rhythm_id: RhythmID) -> Option<&StoredRhythm> {
        self.rhythms.iter().find(|rhythm| rhythm.id == rhythm_id)
    }

    pub fn find_rhythm_mut(&mut self, rhythm_id: RhythmID) -> Option<&mut StoredRhythm> {
        self.rhythms
            .iter_mut()
            .find(|rhythm| rhythm.id == rhythm_id)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.version != CURRENT_DOCUMENT_VERSION {
            return Err(Error::Internal(format!(
                "unsupported cadence document version: {}",
                self.version
            )));
        }

        Tz::from_str(&self.timezone).map_err(|err| Error::Internal(err.to_string()))?;

        let mut seen = HashMap::new();
        for rhythm in &self.rhythms {
            rhythm.validate()?;
            if seen.insert(rhythm.id, ()).is_some() {
                return Err(Error::Internal(format!(
                    "duplicate rhythm id: {}",
                    rhythm.id
                )));
            }
        }

        Ok(())
    }
}

fn default_timezone_string() -> String {
    std::env::var("TZ")
        .ok()
        .filter(|tz| Tz::from_str(tz).is_ok())
        .unwrap_or_else(|| "UTC".to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::TimeZone;

    use super::*;
    use crate::{Rhythm, Slider};

    #[test]
    fn yaml_round_trip_preserves_compact_events() {
        let rhythm_id = RhythmID::generate().unwrap();
        let document = CadenceDocument {
            version: CURRENT_DOCUMENT_VERSION,
            timezone: "UTC".to_string(),
            spoons: BTreeMap::from([(NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(), 4)]),
            rhythms: vec![StoredRhythm {
                id: rhythm_id,
                rhythm: Rhythm::WeekDaily {
                    dotw: 0,
                    at: chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                    slider: Slider::new(1),
                },
                description: "Review backlog".to_string(),
                created_at: Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap(),
                created_at_tz: "UTC".to_string(),
                modified_at: Utc.with_ymd_and_hms(2026, 4, 2, 12, 0, 0).unwrap(),
                modified_at_tz: "UTC".to_string(),
                last_done: Some(StoredEventRecord {
                    when: Utc.with_ymd_and_hms(2026, 4, 10, 12, 0, 0).unwrap(),
                    when_tz: "UTC".to_string(),
                }),
                last_defer: Some(StoredEventRecord {
                    when: Utc.with_ymd_and_hms(2026, 4, 11, 12, 0, 0).unwrap(),
                    when_tz: "UTC".to_string(),
                }),
            }],
        };

        let yaml = serde_yaml::to_string(&document).unwrap();
        let decoded: CadenceDocument = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(decoded, document);
    }

    #[test]
    fn update_from_manager_compacts_events_per_type() {
        let rhythm_id = RhythmID::generate().unwrap();
        let definition = RhythmDefinition {
            id: rhythm_id,
            rhythm: Rhythm::Daily {
                at: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            },
            description: "Daily check".to_string(),
            created_at: Utc.with_ymd_and_hms(2026, 4, 1, 10, 0, 0).unwrap(),
            created_at_tz: "UTC".to_string(),
            modified_at: Utc.with_ymd_and_hms(2026, 4, 1, 10, 0, 0).unwrap(),
            modified_at_tz: "UTC".to_string(),
        };
        let events = vec![
            EventRecord {
                rhythm_id,
                event_type: EventType::Done,
                when: Utc.with_ymd_and_hms(2026, 4, 1, 11, 0, 0).unwrap(),
                when_tz: "UTC".to_string(),
            },
            EventRecord {
                rhythm_id,
                event_type: EventType::Done,
                when: Utc.with_ymd_and_hms(2026, 4, 2, 11, 0, 0).unwrap(),
                when_tz: "UTC".to_string(),
            },
            EventRecord {
                rhythm_id,
                event_type: EventType::Defer,
                when: Utc.with_ymd_and_hms(2026, 4, 3, 11, 0, 0).unwrap(),
                when_tz: "UTC".to_string(),
            },
        ];
        let manager =
            RhythmManager::new(Uuid::nil(), "UTC", vec![definition], events, HashMap::new())
                .unwrap();

        let mut document = CadenceDocument::new("UTC");
        document.update_from_manager(&manager);

        let stored = &document.rhythms[0];
        assert_eq!(
            stored.last_done.as_ref().unwrap().when,
            Utc.with_ymd_and_hms(2026, 4, 2, 11, 0, 0).unwrap()
        );
        assert_eq!(
            stored.last_defer.as_ref().unwrap().when,
            Utc.with_ymd_and_hms(2026, 4, 3, 11, 0, 0).unwrap()
        );
    }
}
