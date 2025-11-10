use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::str::FromStr;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use uuid::Uuid;

use crate::{assign_time, change_time, ONE_DAY};

///////////////////////////////////////////// RhythmID /////////////////////////////////////////////

one_two_eight::generate_id!(RhythmID, "rhythm:");
crate::generate_id_serde!(RhythmID, RhythmIDVisitor);

impl TryFrom<RhythmID> for Uuid {
    type Error = crate::Error;

    fn try_from(rhythm_id: RhythmID) -> Result<Self, Self::Error> {
        let id_str = rhythm_id.to_string();
        let uuid_str = id_str
            .strip_prefix("rhythm:")
            .ok_or_else(|| crate::Error::Internal("Invalid RhythmID format".to_string()))?;
        Uuid::parse_str(uuid_str).map_err(|e| crate::Error::Internal(e.to_string()))
    }
}

impl TryFrom<Uuid> for RhythmID {
    type Error = crate::Error;

    fn try_from(uuid: Uuid) -> Result<Self, Self::Error> {
        let id_str = format!("rhythm:{}", uuid);
        RhythmID::from_human_readable(&id_str)
            .ok_or_else(|| crate::Error::Internal("Failed to parse RhythmID".to_string()))
    }
}

impl TryFrom<&Uuid> for RhythmID {
    type Error = crate::Error;

    fn try_from(uuid: &Uuid) -> Result<Self, Self::Error> {
        Self::try_from(*uuid)
    }
}

////////////////////////////////////////////// Slider //////////////////////////////////////////////

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct Slider {
    pub before: u32,
    pub after: u32,
}

impl Slider {
    pub fn new(before: u32, after: u32) -> Self {
        Self { before, after }
    }
}

////////////////////////////////////////////// Rhythm //////////////////////////////////////////////

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Rhythm {
    Daily {
        at: NaiveTime,
    },
    Monthly {
        dotm: u32,
        at: NaiveTime,
        #[serde(default)]
        slider: Slider,
    },
    WeekDaily {
        dotw: u8,
        at: NaiveTime,
        #[serde(default)]
        slider: Slider,
    },
    EveryNDays {
        n: u32,
        at: NaiveTime,
        #[serde(default)]
        slider: Slider,
    },
}

impl Rhythm {
    pub fn at(self) -> NaiveTime {
        match self {
            Self::Daily { at } => at,
            Self::Monthly { at, .. } => at,
            Self::WeekDaily { at, .. } => at,
            Self::EveryNDays { at, .. } => at,
        }
    }

    pub fn slider(self) -> Slider {
        match self {
            Self::Daily { .. } => Slider::default(),
            Self::Monthly { slider, .. } => slider,
            Self::WeekDaily { slider, .. } => slider,
            Self::EveryNDays { slider, .. } => slider,
        }
    }

    pub fn skip_beat_within_slider(self) -> bool {
        match self {
            Self::Daily { .. } => false,
            Self::Monthly { .. } => true,
            Self::WeekDaily { .. } => true,
            Self::EveryNDays { .. } => false,
        }
    }

    pub fn approximate_periodicity(self) -> u32 {
        match self {
            Self::Daily { .. } => 1,
            Self::Monthly { .. } => 31,
            Self::WeekDaily { .. } => 7,
            Self::EveryNDays { n, .. } => n,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Daily { .. } => "daily",
            Self::Monthly { .. } => "monthly",
            Self::WeekDaily { .. } => "weekdaily",
            Self::EveryNDays { .. } => "every_n_days",
        }
    }

    pub fn next_time_to_fire<TZ: TimeZone + Clone + Copy>(
        self,
        now: DateTime<TZ>,
        last_fired: Option<DateTime<TZ>>,
    ) -> DateTime<TZ> {
        if let Some(last_fired) = last_fired {
            match self {
                Rhythm::Daily { at } => change_time(last_fired, at) + ONE_DAY,
                Rhythm::Monthly { dotm, at, .. } => {
                    let mut next_fire = last_fired;
                    if next_fire.day0() == dotm {
                        next_fire += ONE_DAY;
                    }
                    while next_fire.day0() != dotm {
                        next_fire += ONE_DAY;
                    }
                    change_time(next_fire, at)
                }
                Rhythm::WeekDaily { dotw, at, .. } => {
                    let mut next_fire = last_fired;
                    let dotw = Weekday::try_from(dotw).unwrap_or(Weekday::Mon);
                    if next_fire.weekday() == dotw {
                        next_fire += ONE_DAY;
                    }
                    while next_fire.weekday() != dotw {
                        next_fire += ONE_DAY;
                    }
                    change_time(next_fire, at)
                }
                Rhythm::EveryNDays { n, at, .. } => change_time(last_fired, at) + n * ONE_DAY,
            }
        } else {
            match self {
                Rhythm::Daily { at } => change_time(now, at),
                Rhythm::Monthly { dotm, at, .. } => {
                    let mut next_fire = now.clone();
                    // NOTE(rescrv): Intentionally bail if on this day of the month.
                    while next_fire.day0() != dotm {
                        next_fire += ONE_DAY;
                    }
                    change_time(next_fire, at)
                }
                Rhythm::WeekDaily { dotw, at, .. } => {
                    let mut next_fire = now;
                    let dotw =
                        Weekday::try_from(dotw).expect("no one expects the end of spacetime");
                    // NOTE(rescrv): Intentionally bail if on this day of the month.
                    while next_fire.weekday() != dotw {
                        next_fire += ONE_DAY;
                    }
                    change_time(next_fire, at)
                }
                Rhythm::EveryNDays { n, at, .. } => {
                    if n >= 7 {
                        return now;
                    }
                    let mut date = now.clone() - 7 * ONE_DAY;
                    while date.weekday().num_days_from_monday() != n {
                        date += ONE_DAY;
                    }
                    while date < now.clone() {
                        date = self.next_time_to_fire(now.clone(), Some(date));
                    }
                    change_time(date, at)
                }
            }
        }
    }
}

impl std::fmt::Display for Rhythm {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Daily { at } => write!(f, "daily at {at}"),
            Self::Monthly { dotm, at, slider } => {
                write!(
                    f,
                    "monthly on the {dotm} at {at} slider:{},{}",
                    slider.before, slider.after
                )
            }
            Self::WeekDaily { dotw, at, slider } => {
                let dotw = Weekday::try_from(*dotw).unwrap_or(Weekday::Mon);
                write!(
                    f,
                    "weekly on {dotw} at {at} slider:{},{}",
                    slider.before, slider.after
                )
            }
            Self::EveryNDays { n, at, slider } => {
                write!(
                    f,
                    "every {n} days at {at} slider:{},{}",
                    slider.before, slider.after
                )
            }
        }
    }
}

///////////////////////////////////////// RhythmDefinition /////////////////////////////////////////

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct RhythmDefinition {
    pub id: RhythmID,
    pub rhythm: Rhythm,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub created_at_tz: String,
    pub modified_at: DateTime<Utc>,
    pub modified_at_tz: String,
}

///////////////////////////////////////////// EventType ////////////////////////////////////////////

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Done,
    Defer,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Done => "done",
            EventType::Defer => "defer",
        }
    }
}

impl FromStr for EventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "done" => Ok(EventType::Done),
            "defer" => Ok(EventType::Defer),
            _ => Err(format!("Invalid event type: {s}")),
        }
    }
}

//////////////////////////////////////////// EventRecord ///////////////////////////////////////////

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct EventRecord {
    pub rhythm_id: RhythmID,
    pub event_type: EventType,
    pub when: DateTime<Utc>,
    pub when_tz: String,
}

impl EventRecord {
    pub fn local_time(&self) -> Result<DateTime<Tz>, chrono_tz::ParseError> {
        let tz = Tz::from_str(&self.when_tz)?;
        Ok(self.when.with_timezone(&tz))
    }
}

//////////////////////////////////////////// Smoothing /////////////////////////////////////////////

#[derive(Clone, Debug)]
struct Smoothing {
    rhythm: RhythmDefinition,
    original_beat: NaiveDate,
    remaining_choices: Vec<NaiveDate>,
    passed_over_choices: Vec<NaiveDate>,
}

impl Smoothing {
    fn new(
        rhythm: RhythmDefinition,
        original_beat: NaiveDate,
        start: NaiveDate,
        limit: NaiveDate,
    ) -> Self {
        let mut remaining_choices = Vec::new();
        let slider = rhythm.rhythm.slider();

        // Add before options second (fallback to moving backward)
        for i in 1..=slider.before {
            let date = original_beat - chrono::Duration::days(i as i64);
            if date >= start && date < limit {
                remaining_choices.push(date);
            }
        }

        // Add after options first (prefer moving forward)
        for i in 1..=slider.after {
            let date = original_beat + chrono::Duration::days(i as i64);
            if date >= start && date < limit {
                remaining_choices.push(date);
            }
        }

        Self {
            rhythm,
            original_beat,
            remaining_choices,
            passed_over_choices: Vec::new(),
        }
    }

    fn all_options(&self) -> Vec<NaiveDate> {
        let mut options = vec![self.original_beat];
        let slider = self.rhythm.rhythm.slider();

        for i in 1..=slider.before {
            options.push(self.original_beat - chrono::Duration::days(i as i64));
        }

        for i in 1..=slider.after {
            options.push(self.original_beat + chrono::Duration::days(i as i64));
        }

        options
    }

    fn shift_one(&mut self) -> Option<NaiveDate> {
        if !self.remaining_choices.is_empty() {
            let picked = self.remaining_choices.remove(0);
            self.passed_over_choices.push(picked);
            Some(picked)
        } else {
            None
        }
    }
}

/////////////////////////////////////////// HeapEntry /////////////////////////////////////////////

#[derive(Clone, Debug)]
struct HeapEntry {
    when: DateTime<Tz>,
    num_choices: usize,
    periodicity: u32,
    rhythm_id: RhythmID,
    smoothing: Smoothing,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.when == other.when
            && self.num_choices == other.num_choices
            && self.periodicity == other.periodicity
            && self.rhythm_id == other.rhythm_id
    }
}

impl Eq for HeapEntry {}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.when
            .cmp(&other.when)
            .then_with(|| self.num_choices.cmp(&other.num_choices))
            .then_with(|| self.periodicity.cmp(&other.periodicity))
            .then_with(|| self.rhythm_id.to_string().cmp(&other.rhythm_id.to_string()))
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/////////////////////////////////////////// RhythmManager //////////////////////////////////////////

pub struct RhythmManager {
    user_id: Uuid,
    rhythms: Vec<RhythmDefinition>,
    tz: Tz,
    timezone: String,
    events: Vec<EventRecord>,
    spoons: HashMap<NaiveDate, u8>,
}

impl RhythmManager {
    pub fn new(
        user_id: Uuid,
        timezone: impl AsRef<str>,
        rhythms: Vec<RhythmDefinition>,
        events: Vec<EventRecord>,
        spoons: HashMap<NaiveDate, u8>,
    ) -> Result<Self, crate::Error> {
        let tz =
            Tz::from_str(timezone.as_ref()).map_err(|e| crate::Error::Internal(e.to_string()))?;
        Ok(Self {
            user_id,
            rhythms,
            tz,
            timezone: timezone.as_ref().to_string(),
            events,
            spoons,
        })
    }

    pub fn now(&self) -> DateTime<Tz> {
        Utc::now().with_timezone(&self.tz)
    }

    pub fn set_rhythm(&mut self, mut rhythm: RhythmDefinition) -> Result<(), crate::Error> {
        rhythm.modified_at = Utc::now();
        rhythm.modified_at_tz = self.timezone.clone();
        for r in self.rhythms.iter_mut() {
            if r.id == rhythm.id {
                *r = rhythm;
                return Ok(());
            }
        }
        rhythm.created_at = Utc::now();
        rhythm.created_at_tz = self.timezone.clone();
        self.rhythms.push(rhythm);
        Ok(())
    }

    pub fn unset_rhythm(&mut self, rhythm_id: RhythmID) -> Result<(), crate::Error> {
        self.rhythms.retain(|x| x.id != rhythm_id);
        Ok(())
    }

    pub fn rhythms(&self) -> Vec<RhythmDefinition> {
        self.rhythms.clone()
    }

    pub fn mark_done_at(
        &mut self,
        rhythm_id: RhythmID,
        when: DateTime<Utc>,
    ) -> Result<(), crate::Error> {
        let event = EventRecord {
            rhythm_id,
            event_type: EventType::Done,
            when,
            when_tz: self.timezone.clone(),
        };
        self.events.push(event);
        Ok(())
    }

    pub fn mark_done(&mut self, rhythm_id: RhythmID) -> Result<(), crate::Error> {
        self.mark_done_at(rhythm_id, Utc::now())
    }

    pub fn defer_rhythm_at(
        &mut self,
        rhythm_id: RhythmID,
        when: DateTime<Utc>,
    ) -> Result<(), crate::Error> {
        let event = EventRecord {
            rhythm_id,
            event_type: EventType::Defer,
            when,
            when_tz: self.timezone.clone(),
        };
        self.events.push(event);
        Ok(())
    }

    pub fn defer_rhythm(&mut self, rhythm_id: RhythmID) -> Result<(), crate::Error> {
        self.defer_rhythm_at(rhythm_id, Utc::now())
    }

    pub fn last_done(&self, rhythm_id: RhythmID) -> Option<DateTime<Utc>> {
        self.last_done_time(rhythm_id)
    }

    fn last_done_time(&self, rhythm_id: RhythmID) -> Option<DateTime<Utc>> {
        self.events
            .iter()
            .filter(|e| e.rhythm_id == rhythm_id && e.event_type == EventType::Done)
            .map(|e| e.when)
            .max()
    }

    fn continuing_beat<TZ: TimeZone + Clone + Copy>(
        &self,
        rhythm: &RhythmDefinition,
        start: DateTime<TZ>,
        last_seen: Option<DateTime<TZ>>,
    ) -> DateTime<TZ> {
        if let Some(last_seen) = last_seen {
            let mut beat = rhythm
                .rhythm
                .next_time_to_fire(last_seen.clone(), Some(last_seen.clone()));
            if rhythm.rhythm.skip_beat_within_slider() {
                let days_diff = (beat.date_naive() - last_seen.date_naive()).num_days();
                if days_diff <= rhythm.rhythm.slider().before as i64 {
                    beat = rhythm.rhythm.next_time_to_fire(beat.clone(), Some(beat));
                }
            }
            while beat < start {
                beat = rhythm.rhythm.next_time_to_fire(beat.clone(), Some(beat));
            }
            beat
        } else {
            rhythm.rhythm.next_time_to_fire(start, None)
        }
    }

    pub fn schedule(
        &self,
        start: NaiveDate,
        limit: NaiveDate,
    ) -> Vec<(DateTime<Tz>, RhythmDefinition)> {
        self.schedule_with_capacity_adjustment(start, limit, false)
    }

    pub fn schedule_with_capacity_adjustment(
        &self,
        start: NaiveDate,
        limit: NaiveDate,
        adjust_today_capacity: bool,
    ) -> Vec<(DateTime<Tz>, RhythmDefinition)> {
        self.schedule_internal(start, limit, adjust_today_capacity, &mut HashMap::new())
    }

    fn schedule_internal(
        &self,
        start: NaiveDate,
        limit: NaiveDate,
        adjust_today_capacity: bool,
        watermarks: &mut HashMap<NaiveDate, usize>,
    ) -> Vec<(DateTime<Tz>, RhythmDefinition)> {
        let mut events_by_rhythm: HashMap<RhythmID, Vec<&EventRecord>> = HashMap::new();
        for event in &self.events {
            events_by_rhythm
                .entry(event.rhythm_id)
                .or_default()
                .push(event);
        }

        let today = self.now().date_naive();
        let today_done_deferred_count = if adjust_today_capacity {
            self.events
                .iter()
                .filter(|e| {
                    e.local_time()
                        .map(|dt| dt.date_naive() == today)
                        .unwrap_or(false)
                        && (e.event_type == EventType::Done || e.event_type == EventType::Defer)
                })
                .count()
        } else {
            0
        };

        let mut schedule: HashMap<NaiveDate, Vec<(DateTime<Tz>, RhythmDefinition)>> =
            HashMap::new();

        // NOTE(rescrv):  Daily rhythms have no slider, no smoothing.  They must happen daily.
        // Therefore, unconditionally schedule them for every day in the interval.
        for rhythm in &self.rhythms {
            if let Rhythm::Daily { at } = &rhythm.rhythm {
                let mut day = start;
                while day < limit {
                    let has_done_or_defer = events_by_rhythm
                        .get(&rhythm.id)
                        .map(|events| {
                            events.iter().any(|e| {
                                e.local_time()
                                    .map(|dt| dt.date_naive() == day)
                                    .unwrap_or(false)
                                    && (e.event_type == EventType::Done
                                        || e.event_type == EventType::Defer)
                            })
                        })
                        .unwrap_or(false);

                    if !has_done_or_defer {
                        let when = assign_time(day, *at, self.tz);
                        schedule
                            .entry(day)
                            .or_default()
                            .push((when, rhythm.clone()));
                    }
                    day += chrono::Duration::days(1);
                }
            }
        }

        // Gather non-daily rhythms for heap-based scheduling
        let mut smooth_rhythms = BinaryHeap::new();
        for rhythm in &self.rhythms {
            if matches!(rhythm.rhythm, Rhythm::Daily { .. }) {
                continue;
            }

            let done_events: Vec<DateTime<Utc>> = events_by_rhythm
                .get(&rhythm.id)
                .map(|events| {
                    events
                        .iter()
                        .filter(|e| e.event_type == EventType::Done)
                        .map(|e| e.when)
                        .collect()
                })
                .unwrap_or_default();

            let defer_dates: HashSet<NaiveDate> = events_by_rhythm
                .get(&rhythm.id)
                .map(|events| {
                    events
                        .iter()
                        .filter(|e| e.event_type == EventType::Defer)
                        .filter_map(|e| e.local_time().ok().map(|dt| dt.date_naive()))
                        .collect()
                })
                .unwrap_or_default();

            let last_done = done_events
                .iter()
                .max()
                .cloned()
                .map(|dt| dt.with_timezone(&self.tz));
            let start_dt = start
                .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
                .and_local_timezone(self.tz)
                .earliest()
                .unwrap();

            let mut first_beat = self.continuing_beat(rhythm, start_dt, last_done);
            while defer_dates.contains(&first_beat.date_naive()) {
                first_beat += chrono::Duration::days(1);
            }
            let smoothing = Smoothing::new(rhythm.clone(), first_beat.date_naive(), start, limit);
            let entry = HeapEntry {
                when: first_beat.with_timezone(&self.tz),
                num_choices: smoothing.remaining_choices.len(),
                periodicity: rhythm.rhythm.approximate_periodicity(),
                rhythm_id: rhythm.id,
                smoothing,
            };
            smooth_rhythms.push(Reverse(entry));
        }

        // Process the heap
        while let Some(Reverse(mut entry)) = smooth_rhythms.pop() {
            if entry
                .smoothing
                .all_options()
                .iter()
                .all(|&x| x < start || x >= limit)
            {
                continue;
            }

            let slots_per_day = watermarks
                .get(&entry.when.date_naive())
                .copied()
                .unwrap_or(1);
            let spoons_today = self
                .spoons
                .get(&entry.when.date_naive())
                .copied()
                .unwrap_or(5);
            let spoons_adjusted = (spoons_today as i32 - 5).clamp(-5, 5);
            let slots_per_day =
                ((slots_per_day as f64) * 2_f64.powf(spoons_adjusted as f64 / 5.0)).ceil() as usize;

            let slots_per_day = if entry.when.date_naive() == today && adjust_today_capacity {
                slots_per_day.saturating_sub(today_done_deferred_count)
            } else {
                slots_per_day
            };

            let slots = schedule.entry(entry.when.date_naive()).or_default();

            let defer_today = self.events.iter().any(|e| {
                e.rhythm_id == entry.rhythm_id
                    && e.event_type == EventType::Defer
                    && e.local_time()
                        .map(|dt| dt.date_naive() == entry.when.date_naive())
                        .unwrap_or(false)
            });

            if !defer_today && slots.len() < slots_per_day {
                slots.push((entry.when, entry.smoothing.rhythm.clone()));
                let next_beat =
                    self.continuing_beat(&entry.smoothing.rhythm, entry.when, Some(entry.when));
                let new_smoothing = Smoothing::new(
                    entry.smoothing.rhythm.clone(),
                    next_beat.date_naive(),
                    start,
                    limit,
                );

                let new_entry = HeapEntry {
                    when: next_beat,
                    num_choices: new_smoothing.remaining_choices.len(),
                    periodicity: entry.periodicity,
                    rhythm_id: entry.rhythm_id,
                    smoothing: new_smoothing,
                };
                smooth_rhythms.push(Reverse(new_entry));
            } else if defer_today || !entry.smoothing.remaining_choices.is_empty() {
                // Try to reschedule
                let next_day = if defer_today
                    && !entry
                        .smoothing
                        .remaining_choices
                        .contains(&(entry.when.date_naive() + chrono::Duration::days(1)))
                {
                    entry.when.date_naive() + chrono::Duration::days(1)
                } else if let Some(next) = entry.smoothing.shift_one() {
                    next
                } else {
                    // TODO(rescrv):  Make this an explicit error.
                    continue;
                };

                entry.when = assign_time(
                    next_day,
                    self.rhythms
                        .iter()
                        .find(|r| r.id == entry.smoothing.rhythm.id)
                        .map(|x| x.rhythm.at())
                        .unwrap(),
                    self.tz,
                );
                entry.num_choices = entry.smoothing.remaining_choices.len();
                smooth_rhythms.push(Reverse(entry));
            } else {
                let options = entry.smoothing.all_options();
                let mut low_water_mark = None;

                for date in &options {
                    if let Some(&wm) = watermarks.get(date) {
                        low_water_mark = Some(low_water_mark.map_or(wm, |lwm: usize| lwm.min(wm)));
                    } else {
                        low_water_mark = Some(0);
                    }
                }

                if adjust_today_capacity {
                    let wm = watermarks.entry(entry.when.date_naive()).or_default();
                    *wm += today_done_deferred_count;
                }

                let low_water_mark = low_water_mark.unwrap_or(0) + 1;

                for date in options {
                    let current = watermarks.get(&date).copied().unwrap_or(low_water_mark);
                    watermarks.insert(date, current.max(low_water_mark));
                }

                return self.schedule_internal(start, limit, adjust_today_capacity, watermarks);
            }
        }
        let mut result = schedule.values().flatten().cloned().collect::<Vec<_>>();
        result.sort_by_key(|x| x.0);
        result
    }

    pub fn active(&self) -> Vec<(DateTime<Tz>, RhythmDefinition)> {
        let now = Utc::now();
        let mut schedule = self.schedule(
            now.date_naive(),
            (now + chrono::Duration::days(90)).date_naive(),
        );
        schedule.retain(|x| x.0 < now);
        schedule
    }

    pub fn convergence(&self) -> DateTime<Utc> {
        // Find the maximum time any rhythm will fire in the future
        let now = Utc::now();
        let mut max_fire_time = now;

        for rhythm in &self.rhythms {
            // Get the last done event for this rhythm
            let last_done = self
                .events
                .iter()
                .filter(|e| e.rhythm_id == rhythm.id && e.event_type == EventType::Done)
                .map(|e| e.when)
                .max();

            // Calculate next fire time
            let next_fire = rhythm.rhythm.next_time_to_fire(now, last_done);
            if next_fire > max_fire_time {
                max_fire_time = next_fire;
            }
        }

        max_fire_time
    }

    pub fn delinquent(&self) -> Vec<RhythmDefinition> {
        let now = Utc::now();
        let mut delinquent_rhythms = Vec::new();

        for rhythm in &self.rhythms {
            // Get the last done event for this rhythm
            let last_done = self
                .events
                .iter()
                .filter(|e| e.rhythm_id == rhythm.id && e.event_type == EventType::Done)
                .map(|e| e.when)
                .max();

            // Check if the rhythm should have fired by now
            let next_fire = rhythm.rhythm.next_time_to_fire(
                last_done.unwrap_or(now - chrono::Duration::days(365)),
                last_done,
            );

            // If it should have fired before now, it's delinquent
            if next_fire < now {
                delinquent_rhythms.push(rhythm.clone());
            }
        }

        delinquent_rhythms
    }

    pub fn spoons(&mut self, day: impl Datelike, spoons: u8) {
        let date = NaiveDate::from_ymd_opt(day.year(), day.month(), day.day()).unwrap();
        self.spoons.insert(date, spoons.min(10));
    }

    pub fn show_spoons(&self) -> HashMap<NaiveDate, u8> {
        self.spoons.clone()
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn events(&self) -> &[EventRecord] {
        &self.events
    }

    pub fn timezone(&self) -> &str {
        &self.timezone
    }
}

/////////////////////////////////////////////// tests //////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use chrono::{NaiveTime, TimeZone, Utc};
    use chrono_tz::America::Los_Angeles;

    use super::*;

    const TEST_AT: NaiveTime = NaiveTime::from_hms_opt(13, 37, 42).unwrap();

    fn create_test_manager(timezone: &str) -> RhythmManager {
        RhythmManager::new(
            Uuid::new_v4(),
            timezone,
            Vec::new(),
            Vec::new(),
            HashMap::new(),
        )
        .unwrap()
    }

    #[test]
    fn daily_start_beat() {
        let daily = Rhythm::Daily { at: TEST_AT };
        let now = Utc.with_ymd_and_hms(2023, 8, 19, 0, 0, 0).unwrap();
        let result = daily.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2023, 8, 19, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn daily_next_beat() {
        let daily = Rhythm::Daily { at: TEST_AT };
        let now = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let last_fired = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = daily.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 11, 21, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn monthly_start_beat_same_day() {
        let monthly = Rhythm::Monthly {
            dotm: 17,
            at: TEST_AT,
            slider: Slider::default(),
        }; // 18th day (0-based)
        let now = Utc.with_ymd_and_hms(2022, 11, 18, 0, 0, 0).unwrap();
        let result = monthly.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 11, 18, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn monthly_start_beat_different_day() {
        let monthly = Rhythm::Monthly {
            dotm: 17,
            at: TEST_AT,
            slider: Slider::default(),
        }; // 18th day (0-based)
        let now = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = monthly.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 12, 18, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn monthly_next_beat_same_day() {
        let monthly = Rhythm::Monthly {
            dotm: 17,
            at: TEST_AT,
            slider: Slider::default(),
        }; // 18th day (0-based)
        let now = Utc.with_ymd_and_hms(2022, 11, 18, 0, 0, 0).unwrap();
        let last_fired = Utc.with_ymd_and_hms(2022, 11, 18, 0, 0, 0).unwrap();
        let result = monthly.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 12, 18, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn monthly_next_beat_different_day() {
        let monthly = Rhythm::Monthly {
            dotm: 17,
            at: TEST_AT,
            slider: Slider::default(),
        }; // 18th day (0-based)
        let now = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let last_fired = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = monthly.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 12, 18, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn week_daily_start_beat_same_day() {
        let week_daily = Rhythm::WeekDaily {
            dotw: 5,
            at: TEST_AT,
            slider: Slider::default(),
        }; // Saturday
        let now = Utc.with_ymd_and_hms(2022, 11, 19, 0, 0, 0).unwrap(); // Saturday
        let result = week_daily.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 11, 19, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn week_daily_start_beat_different_day() {
        let week_daily = Rhythm::WeekDaily {
            dotw: 5,
            at: TEST_AT,
            slider: Slider::default(),
        }; // Saturday
        let now = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap(); // Sunday
        let result = week_daily.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 11, 26, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn week_daily_next_beat_same_day() {
        let week_daily = Rhythm::WeekDaily {
            dotw: 5,
            at: TEST_AT,
            slider: Slider::default(),
        }; // Saturday
        let now = Utc.with_ymd_and_hms(2022, 11, 19, 0, 0, 0).unwrap();
        let last_fired = Utc.with_ymd_and_hms(2022, 11, 19, 0, 0, 0).unwrap();
        let result = week_daily.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 11, 26, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn week_daily_next_beat_different_day() {
        let week_daily = Rhythm::WeekDaily {
            dotw: 5,
            at: TEST_AT,
            slider: Slider::default(),
        }; // Saturday
        let now = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let last_fired = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = week_daily.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 11, 26, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn every_n_days_start_beat() {
        let every_n_days = Rhythm::EveryNDays {
            n: 5,
            at: TEST_AT,
            slider: Slider::default(),
        };
        let now = Utc.with_ymd_and_hms(2023, 8, 19, 0, 0, 0).unwrap();
        let result = every_n_days.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2023, 8, 22, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn every_n_days_next_beat() {
        let every_n_days = Rhythm::EveryNDays {
            n: 5,
            at: TEST_AT,
            slider: Slider::default(),
        };
        let now = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let last_fired = Utc.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = every_n_days.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Utc.with_ymd_and_hms(2022, 11, 25, 13, 37, 42).unwrap()
        );
    }

    #[test]
    fn daily_start_beat_la() {
        let daily = Rhythm::Daily { at: TEST_AT };
        let now = Los_Angeles.with_ymd_and_hms(2023, 8, 19, 0, 0, 0).unwrap();
        let result = daily.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2023, 8, 19, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn daily_next_beat_la() {
        let daily = Rhythm::Daily { at: TEST_AT };
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let last_fired = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = daily.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 11, 21, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn monthly_start_beat_same_day_la() {
        let monthly = Rhythm::Monthly {
            dotm: 17,
            at: TEST_AT,
            slider: Slider::default(),
        }; // 18th day (0-based)
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 18, 0, 0, 0).unwrap();
        let result = monthly.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 11, 18, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn monthly_start_beat_different_day_la() {
        let monthly = Rhythm::Monthly {
            dotm: 17,
            at: TEST_AT,
            slider: Slider::default(),
        }; // 18th day (0-based)
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = monthly.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 12, 18, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn monthly_next_beat_same_day_la() {
        let monthly = Rhythm::Monthly {
            dotm: 17,
            at: TEST_AT,
            slider: Slider::default(),
        }; // 18th day (0-based)
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 18, 0, 0, 0).unwrap();
        let last_fired = Los_Angeles.with_ymd_and_hms(2022, 11, 18, 0, 0, 0).unwrap();
        let result = monthly.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 12, 18, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn monthly_next_beat_different_day_la() {
        let monthly = Rhythm::Monthly {
            dotm: 17,
            at: TEST_AT,
            slider: Slider::default(),
        }; // 18th day (0-based)
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let last_fired = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = monthly.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 12, 18, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn week_daily_start_beat_same_day_la() {
        let week_daily = Rhythm::WeekDaily {
            dotw: 5,
            at: TEST_AT,
            slider: Slider::default(),
        }; // Saturday
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 19, 0, 0, 0).unwrap(); // Saturday
        let result = week_daily.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 11, 19, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn week_daily_start_beat_different_day_la() {
        let week_daily = Rhythm::WeekDaily {
            dotw: 5,
            at: TEST_AT,
            slider: Slider::default(),
        }; // Saturday
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap(); // Sunday
        let result = week_daily.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 11, 26, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn week_daily_next_beat_same_day_la() {
        let week_daily = Rhythm::WeekDaily {
            dotw: 5,
            at: TEST_AT,
            slider: Slider::default(),
        }; // Saturday
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 19, 0, 0, 0).unwrap();
        let last_fired = Los_Angeles.with_ymd_and_hms(2022, 11, 19, 0, 0, 0).unwrap();
        let result = week_daily.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 11, 26, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn week_daily_next_beat_different_day_la() {
        let week_daily = Rhythm::WeekDaily {
            dotw: 5,
            at: TEST_AT,
            slider: Slider::default(),
        }; // Saturday
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let last_fired = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = week_daily.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 11, 26, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn every_n_days_start_beat_la() {
        let every_n_days = Rhythm::EveryNDays {
            n: 5,
            at: TEST_AT,
            slider: Slider::default(),
        };
        let now = Los_Angeles.with_ymd_and_hms(2023, 8, 19, 0, 0, 0).unwrap();
        let result = every_n_days.next_time_to_fire(now, None);
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2023, 8, 22, 13, 37, 42)
                .unwrap()
        );
    }

    #[test]
    fn every_n_days_next_beat_la() {
        let every_n_days = Rhythm::EveryNDays {
            n: 5,
            at: TEST_AT,
            slider: Slider::default(),
        };
        let now = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let last_fired = Los_Angeles.with_ymd_and_hms(2022, 11, 20, 0, 0, 0).unwrap();
        let result = every_n_days.next_time_to_fire(now, Some(last_fired));
        assert_eq!(
            result,
            Los_Angeles
                .with_ymd_and_hms(2022, 11, 25, 13, 37, 42)
                .unwrap()
        );
    }

    fn create_test_rhythm(id: &str, rhythm: Rhythm) -> RhythmDefinition {
        RhythmDefinition {
            id: RhythmID::generate().unwrap(),
            rhythm,
            description: format!("Test rhythm {id}"),
            created_at: Utc::now(),
            created_at_tz: "UTC".to_string(),
            modified_at: Utc::now(),
            modified_at_tz: "UTC".to_string(),
        }
    }

    fn group_schedule_by_date(
        schedule: Vec<(DateTime<Tz>, RhythmDefinition)>,
    ) -> HashMap<NaiveDate, Vec<RhythmDefinition>> {
        let mut grouped = HashMap::new();
        for (dt, rhythm) in schedule {
            grouped
                .entry(dt.date_naive())
                .or_insert_with(Vec::new)
                .push(rhythm);
        }
        grouped
    }

    #[test]
    fn schedule_daily_rhythm() {
        let mut manager = create_test_manager("America/Los_Angeles");
        let daily = create_test_rhythm("daily1", Rhythm::Daily { at: TEST_AT });
        manager.set_rhythm(daily.clone()).unwrap();

        let start = NaiveDate::from_ymd_opt(2023, 8, 19).unwrap();
        let limit = NaiveDate::from_ymd_opt(2023, 8, 26).unwrap();
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        // Daily should appear every day
        for day in 0..7 {
            let date = start + chrono::Duration::days(day);
            assert!(schedule.contains_key(&date));
            let rhythms = &schedule[&date];
            assert_eq!(rhythms.len(), 1);
            assert_eq!(rhythms[0].id, daily.id);
        }
    }

    #[test]
    fn schedule_monthly_rhythm() {
        let mut manager = create_test_manager("America/Los_Angeles");
        let monthly = create_test_rhythm(
            "monthly1",
            Rhythm::Monthly {
                dotm: 14, // 15th day (0-based)
                at: TEST_AT,
                slider: Slider::default(),
            },
        );
        manager.set_rhythm(monthly.clone()).unwrap();

        let start = NaiveDate::from_ymd_opt(2023, 8, 1).unwrap();
        let limit = NaiveDate::from_ymd_opt(2023, 10, 1).unwrap();
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        // Should appear on the 15th of August and September
        let aug_15 = NaiveDate::from_ymd_opt(2023, 8, 15).unwrap();
        let sep_15 = NaiveDate::from_ymd_opt(2023, 9, 15).unwrap();

        assert!(schedule.contains_key(&aug_15));
        assert!(schedule.contains_key(&sep_15));

        let aug_rhythms = &schedule[&aug_15];
        assert_eq!(aug_rhythms.len(), 1);
        assert_eq!(aug_rhythms[0].id, monthly.id);

        let sep_rhythms = &schedule[&sep_15];
        assert_eq!(sep_rhythms.len(), 1);
        assert_eq!(sep_rhythms[0].id, monthly.id);
    }

    #[test]
    fn schedule_weekly_rhythm() {
        let mut manager = create_test_manager("America/Los_Angeles");
        let weekly = create_test_rhythm(
            "weekly1",
            Rhythm::WeekDaily {
                dotw: 1, // Tuesday
                at: TEST_AT,
                slider: Slider::default(),
            },
        );
        manager.set_rhythm(weekly.clone()).unwrap();

        let start = NaiveDate::from_ymd_opt(2023, 8, 14).unwrap(); // Monday
        let limit = NaiveDate::from_ymd_opt(2023, 8, 28).unwrap(); // Two weeks
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        // Should appear on Tuesdays: Aug 15 and Aug 22
        let tuesday1 = NaiveDate::from_ymd_opt(2023, 8, 15).unwrap();
        let tuesday2 = NaiveDate::from_ymd_opt(2023, 8, 22).unwrap();

        assert!(schedule.contains_key(&tuesday1));
        assert!(schedule.contains_key(&tuesday2));

        let rhythms1 = &schedule[&tuesday1];
        assert_eq!(rhythms1.len(), 1);
        assert_eq!(rhythms1[0].id, weekly.id);

        let rhythms2 = &schedule[&tuesday2];
        assert_eq!(rhythms2.len(), 1);
        assert_eq!(rhythms2[0].id, weekly.id);
    }

    #[test]
    fn schedule_every_n_days_rhythm() {
        let mut manager = create_test_manager("America/Los_Angeles");
        let every3 = create_test_rhythm(
            "every3",
            Rhythm::EveryNDays {
                n: 3,
                at: TEST_AT,
                slider: Slider::default(),
            },
        );
        manager.set_rhythm(every3.clone()).unwrap();

        let start = NaiveDate::from_ymd_opt(2023, 8, 19).unwrap();
        let limit = NaiveDate::from_ymd_opt(2023, 8, 29).unwrap();
        let schedule_vec = manager.schedule(start, limit);

        // Should appear every 3 days starting from a predictable day
        // Based on the algorithm, it aligns to weekday for n<7
        let mut found_dates = Vec::new();
        for (dt, rhythm) in &schedule_vec {
            if rhythm.id == every3.id {
                found_dates.push(dt.date_naive());
            }
        }
        found_dates.sort();

        // Verify spacing between dates
        for i in 1..found_dates.len() {
            let diff = (found_dates[i] - found_dates[i - 1]).num_days();
            assert_eq!(diff, 3);
        }
    }

    #[test]
    fn schedule_with_done_event() {
        let mut manager = create_test_manager("America/Los_Angeles");
        let daily = create_test_rhythm("daily2", Rhythm::Daily { at: TEST_AT });
        manager.set_rhythm(daily.clone()).unwrap();

        // Mark today as done
        let today = NaiveDate::from_ymd_opt(2023, 8, 20).unwrap();
        manager.events.push(EventRecord {
            rhythm_id: daily.id,
            event_type: EventType::Done,
            when: today
                .and_time(TEST_AT)
                .and_local_timezone(Utc)
                .earliest()
                .unwrap(),
            when_tz: "UTC".to_string(),
        });

        let start = NaiveDate::from_ymd_opt(2023, 8, 19).unwrap();
        let limit = NaiveDate::from_ymd_opt(2023, 8, 22).unwrap();
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        // Should not appear on the day marked as done
        let empty_vec = Vec::new();
        let today_rhythms = schedule.get(&today).unwrap_or(&empty_vec);
        assert_eq!(today_rhythms.len(), 0);

        // Should appear on other days
        let yesterday = today - chrono::Duration::days(1);
        let tomorrow = today + chrono::Duration::days(1);

        assert!(schedule
            .get(&yesterday)
            .unwrap_or(&empty_vec)
            .iter()
            .any(|r| r.id == daily.id));
        assert!(schedule
            .get(&tomorrow)
            .unwrap_or(&empty_vec)
            .iter()
            .any(|r| r.id == daily.id));
    }

    #[test]
    fn schedule_with_defer_event() {
        let mut manager = create_test_manager("America/Los_Angeles");
        let weekly = create_test_rhythm(
            "weekly2",
            Rhythm::WeekDaily {
                dotw: 3, // Thursday
                at: TEST_AT,
                slider: Slider::new(1, 1), // Can move 1 day before or after
            },
        );
        manager.set_rhythm(weekly.clone()).unwrap();

        // Defer Thursday Aug 17
        let deferred_day = NaiveDate::from_ymd_opt(2023, 8, 17).unwrap();
        manager.events.push(EventRecord {
            rhythm_id: weekly.id,
            event_type: EventType::Defer,
            when: deferred_day
                .and_time(TEST_AT)
                .and_local_timezone(Utc)
                .earliest()
                .unwrap(),
            when_tz: "UTC".to_string(),
        });

        let start = NaiveDate::from_ymd_opt(2023, 8, 14).unwrap();
        let limit = NaiveDate::from_ymd_opt(2023, 8, 21).unwrap();
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        // Should not appear on deferred day
        let empty_vec = Vec::new();
        let deferred_day_rhythms = schedule.get(&deferred_day).unwrap_or(&empty_vec);
        assert!(!deferred_day_rhythms.iter().any(|r| r.id == weekly.id));

        // Should appear on an adjacent day due to slider
        let day_before = deferred_day - chrono::Duration::days(1);
        let day_after = deferred_day + chrono::Duration::days(1);

        let appears_before = schedule
            .get(&day_before)
            .map(|rhythms| rhythms.iter().any(|r| r.id == weekly.id))
            .unwrap_or(false);
        let appears_after = schedule
            .get(&day_after)
            .map(|rhythms| rhythms.iter().any(|r| r.id == weekly.id))
            .unwrap_or(false);

        assert!(
            appears_before || appears_after,
            "Weekly rhythm should appear on an adjacent day due to slider"
        );
    }

    #[test]
    fn schedule_with_slider() {
        let mut manager = create_test_manager("America/Los_Angeles");

        // Add two monthly rhythms that would normally conflict
        let monthly1 = create_test_rhythm(
            "monthly_slider1",
            Rhythm::Monthly {
                dotm: 14, // 15th
                at: TEST_AT,
                slider: Slider::new(2, 2), // Can move 2 days either way
            },
        );
        let monthly2 = create_test_rhythm(
            "monthly_slider2",
            Rhythm::Monthly {
                dotm: 14, // Also 15th
                at: TEST_AT,
                slider: Slider::new(2, 2),
            },
        );

        manager.set_rhythm(monthly1.clone()).unwrap();
        manager.set_rhythm(monthly2.clone()).unwrap();

        let start = NaiveDate::from_ymd_opt(2023, 8, 10).unwrap();
        let limit = NaiveDate::from_ymd_opt(2023, 8, 20).unwrap();
        let schedule_vec = manager.schedule(start, limit);

        // Both should be scheduled, potentially on different days due to smoothing
        let mut found1 = false;
        let mut found2 = false;

        for (_, rhythm) in &schedule_vec {
            if rhythm.id == monthly1.id {
                found1 = true;
            }
            if rhythm.id == monthly2.id {
                found2 = true;
            }
        }

        assert!(found1);
        assert!(found2);
    }

    #[test]
    fn schedule_with_spoons() {
        let mut manager = create_test_manager("America/Los_Angeles");

        // Add multiple daily rhythms
        for i in 0..5 {
            let daily =
                create_test_rhythm(&format!("daily_spoons_{i}"), Rhythm::Daily { at: TEST_AT });
            manager.set_rhythm(daily).unwrap();
        }

        let test_day = NaiveDate::from_ymd_opt(2023, 8, 20).unwrap();

        // Set low spoons (should reduce capacity)
        manager.spoons(test_day, 2);

        let start = NaiveDate::from_ymd_opt(2023, 8, 19).unwrap();
        let limit = NaiveDate::from_ymd_opt(2023, 8, 22).unwrap();
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        // All 5 dailies should still appear (dailies bypass spoons system)
        let empty_vec = Vec::new();
        let test_day_rhythms = schedule.get(&test_day).unwrap_or(&empty_vec);
        assert_eq!(test_day_rhythms.len(), 5);
    }

    #[test]
    fn convergence_test() {
        let mut manager = create_test_manager("America/Los_Angeles");

        let weekly = create_test_rhythm(
            "conv_weekly",
            Rhythm::WeekDaily {
                dotw: 1, // Tuesday
                at: TEST_AT,
                slider: Slider::default(),
            },
        );
        let monthly = create_test_rhythm(
            "conv_monthly",
            Rhythm::Monthly {
                dotm: 24, // 25th
                at: TEST_AT,
                slider: Slider::default(),
            },
        );

        manager.set_rhythm(weekly).unwrap();
        manager.set_rhythm(monthly).unwrap();

        // Mark weekly as done recently
        manager.events.push(EventRecord {
            rhythm_id: manager.rhythms[0].id,
            event_type: EventType::Done,
            when: Utc::now() - chrono::Duration::days(1),
            when_tz: "UTC".to_string(),
        });

        let convergence = manager.convergence();

        // Convergence should be in the future
        assert!(convergence > Utc::now());

        // Should be within a year
        assert!(convergence < Utc::now() + chrono::Duration::days(365));
    }

    #[test]
    fn delinquent_test() {
        let mut manager = create_test_manager("America/Los_Angeles");

        let daily1 = create_test_rhythm("delinquent1", Rhythm::Daily { at: TEST_AT });
        let daily2 = create_test_rhythm("delinquent2", Rhythm::Daily { at: TEST_AT });

        manager.set_rhythm(daily1.clone()).unwrap();
        manager.set_rhythm(daily2.clone()).unwrap();

        // Mark daily1 as done today (not delinquent)
        manager.events.push(EventRecord {
            rhythm_id: daily1.id,
            event_type: EventType::Done,
            when: Utc::now() - chrono::Duration::hours(1),
            when_tz: "UTC".to_string(),
        });

        // Mark daily2 as done 3 days ago (delinquent)
        manager.events.push(EventRecord {
            rhythm_id: daily2.id,
            event_type: EventType::Done,
            when: Utc::now() - chrono::Duration::days(3),
            when_tz: "UTC".to_string(),
        });

        let delinquent = manager.delinquent();

        // Only daily2 should be delinquent
        assert_eq!(delinquent.len(), 1);
        assert_eq!(delinquent[0].id, daily2.id);
    }

    #[test]
    fn complex_mixed_schedule() {
        let mut manager = create_test_manager("America/Los_Angeles");

        // Add various rhythm types
        let daily = create_test_rhythm("mixed_daily", Rhythm::Daily { at: TEST_AT });
        let weekly = create_test_rhythm(
            "mixed_weekly",
            Rhythm::WeekDaily {
                dotw: 3, // Thursday
                at: TEST_AT,
                slider: Slider::new(1, 1),
            },
        );
        let monthly = create_test_rhythm(
            "mixed_monthly",
            Rhythm::Monthly {
                dotm: 9, // 10th
                at: TEST_AT,
                slider: Slider::new(2, 2),
            },
        );
        let every5 = create_test_rhythm(
            "mixed_every5",
            Rhythm::EveryNDays {
                n: 5,
                at: TEST_AT,
                slider: Slider::new(1, 1),
            },
        );

        manager.set_rhythm(daily.clone()).unwrap();
        manager.set_rhythm(weekly.clone()).unwrap();
        manager.set_rhythm(monthly.clone()).unwrap();
        manager.set_rhythm(every5.clone()).unwrap();

        // Add some events
        let base_date = NaiveDate::from_ymd_opt(2023, 8, 8).unwrap();

        // Mark daily as done on Aug 8
        manager.events.push(EventRecord {
            rhythm_id: daily.id,
            event_type: EventType::Done,
            when: base_date
                .and_time(TEST_AT)
                .and_local_timezone(Utc)
                .earliest()
                .unwrap(),
            when_tz: "UTC".to_string(),
        });

        // Defer weekly on Aug 10 (Thursday)
        let thursday = NaiveDate::from_ymd_opt(2023, 8, 10).unwrap();
        manager.events.push(EventRecord {
            rhythm_id: weekly.id,
            event_type: EventType::Defer,
            when: thursday
                .and_time(TEST_AT)
                .and_local_timezone(Utc)
                .earliest()
                .unwrap(),
            when_tz: "UTC".to_string(),
        });

        let start = NaiveDate::from_ymd_opt(2023, 8, 7).unwrap();
        let limit = NaiveDate::from_ymd_opt(2023, 8, 14).unwrap();
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec.clone());

        // Verify daily appears every day except Aug 8
        for day in 0..7 {
            let date = start + chrono::Duration::days(day);
            let has_daily = schedule
                .get(&date)
                .map(|rhythms| rhythms.iter().any(|r| r.id == daily.id))
                .unwrap_or(false);

            if date == base_date {
                assert!(!has_daily, "Daily should not appear on done day");
            } else {
                assert!(has_daily, "Daily should appear on {date}");
            }
        }

        // Verify weekly doesn't appear on deferred Thursday
        let empty_vec = Vec::new();
        let thursday_rhythms = schedule.get(&thursday).unwrap_or(&empty_vec);
        assert!(!thursday_rhythms.iter().any(|r| r.id == weekly.id));

        // Verify monthly appears on Aug 9 (moved from Aug 10 to distribute load)
        let ninth = NaiveDate::from_ymd_opt(2023, 8, 9).unwrap();
        let ninth_rhythms = schedule.get(&ninth);
        assert!(ninth_rhythms.is_some());
        assert!(ninth_rhythms.unwrap().iter().any(|r| r.id == monthly.id));

        // Verify every5 appears somewhere
        let mut every5_found = false;
        for (_, rhythm) in &schedule_vec {
            if rhythm.id == every5.id {
                every5_found = true;
                break;
            }
        }
        assert!(every5_found);
    }

    #[test]
    fn capacity_adjustment_with_done_and_deferred() {
        let mut manager = create_test_manager("America/Los_Angeles");

        let rhythm1 = create_test_rhythm(
            "rhythm1_capacity",
            Rhythm::EveryNDays {
                n: 2,
                at: TEST_AT,
                slider: Slider::new(1, 1),
            },
        );
        let rhythm2 = create_test_rhythm(
            "rhythm2_capacity",
            Rhythm::EveryNDays {
                n: 3,
                at: TEST_AT,
                slider: Slider::new(1, 1),
            },
        );

        manager.set_rhythm(rhythm1.clone()).unwrap();
        manager.set_rhythm(rhythm2.clone()).unwrap();

        let today = manager.now().date_naive();

        manager.events.push(EventRecord {
            rhythm_id: rhythm1.id,
            event_type: EventType::Done,
            when: manager.now().with_timezone(&Utc),
            when_tz: manager.timezone().to_string(),
        });

        let schedule_without_adjustment =
            manager.schedule(today, today + chrono::Duration::days(10));

        let schedule_with_adjustment = manager.schedule_with_capacity_adjustment(
            today,
            today + chrono::Duration::days(10),
            true,
        );

        let today_count_without = schedule_without_adjustment
            .iter()
            .filter(|(dt, _)| dt.date_naive() == today)
            .count();
        let today_count_with = schedule_with_adjustment
            .iter()
            .filter(|(dt, _)| dt.date_naive() == today)
            .count();

        println!("Today count without adjustment: {today_count_without}");
        println!("Today count with adjustment: {today_count_with}");
        println!(
            "Schedule without adjustment: {:?}",
            schedule_without_adjustment
                .iter()
                .map(|(dt, r)| (dt.date_naive(), &r.description))
                .collect::<Vec<_>>()
        );
        println!(
            "Schedule with adjustment: {:?}",
            schedule_with_adjustment
                .iter()
                .map(|(dt, r)| (dt.date_naive(), &r.description))
                .collect::<Vec<_>>()
        );

        assert!(
            today_count_with < today_count_without
                || (today_count_without == 0 && today_count_with == 0),
            "With adjustment, today should have fewer or equal tasks. Without: {today_count_without}, With: {today_count_with}"
        );
    }

    #[test]
    fn stretch_goals_identification() {
        let mut manager = create_test_manager("America/Los_Angeles");

        let rhythm1 = create_test_rhythm(
            "rhythm1_stretch",
            Rhythm::EveryNDays {
                n: 2,
                at: TEST_AT,
                slider: Slider::new(1, 1),
            },
        );
        let rhythm2 = create_test_rhythm(
            "rhythm2_stretch",
            Rhythm::EveryNDays {
                n: 3,
                at: TEST_AT,
                slider: Slider::new(1, 1),
            },
        );
        let rhythm3 = create_test_rhythm(
            "rhythm3_stretch",
            Rhythm::EveryNDays {
                n: 4,
                at: TEST_AT,
                slider: Slider::new(1, 1),
            },
        );

        manager.set_rhythm(rhythm1.clone()).unwrap();
        manager.set_rhythm(rhythm2.clone()).unwrap();
        manager.set_rhythm(rhythm3.clone()).unwrap();

        let today = manager.now().date_naive();
        manager.events.push(EventRecord {
            rhythm_id: rhythm1.id,
            event_type: EventType::Done,
            when: manager.now().with_timezone(&Utc),
            when_tz: manager.timezone().to_string(),
        });

        let limit = today + chrono::Duration::days(90);

        let schedule_without_adjustment = manager.schedule(today, limit);
        let schedule_with_adjustment =
            manager.schedule_with_capacity_adjustment(today, limit, true);

        let today_without: Vec<_> = schedule_without_adjustment
            .iter()
            .filter(|(dt, _)| dt.date_naive() == today)
            .collect();
        let today_with: Vec<_> = schedule_with_adjustment
            .iter()
            .filter(|(dt, _)| dt.date_naive() == today)
            .collect();

        println!("Today without adjustment: {}", today_without.len());
        println!("Today with adjustment: {}", today_with.len());

        assert!(
            today_without.len() >= today_with.len(),
            "Schedule without adjustment should have more or equal tasks for today"
        );

        let with_ids: std::collections::HashSet<_> =
            today_with.iter().map(|(_, rhythm)| rhythm.id).collect();

        let stretch_goals: Vec<_> = today_without
            .iter()
            .filter(|(_, rhythm)| !with_ids.contains(&rhythm.id))
            .collect();

        println!("Stretch goals: {}", stretch_goals.len());

        if today_without.len() > today_with.len() {
            assert!(
                !stretch_goals.is_empty(),
                "There should be stretch goals when schedules differ"
            );
        }
    }

    #[test]
    fn timezone_boundary_done_defer_count() {
        let mut manager = create_test_manager("America/Los_Angeles");

        let daily = create_test_rhythm("tz_daily", Rhythm::Daily { at: TEST_AT });
        manager.set_rhythm(daily.clone()).unwrap();

        let la_date = NaiveDate::from_ymd_opt(2023, 8, 20).unwrap();
        let la_time = NaiveTime::from_hms_opt(23, 30, 0).unwrap();
        let la_datetime = la_date
            .and_time(la_time)
            .and_local_timezone(Los_Angeles)
            .earliest()
            .unwrap();

        manager.events.push(EventRecord {
            rhythm_id: daily.id,
            event_type: EventType::Done,
            when: la_datetime.with_timezone(&Utc),
            when_tz: "America/Los_Angeles".to_string(),
        });

        let schedule_vec = manager.schedule_with_capacity_adjustment(
            la_date,
            la_date + chrono::Duration::days(2),
            true,
        );
        let schedule = group_schedule_by_date(schedule_vec);

        let empty_vec = Vec::new();
        let today_rhythms = schedule.get(&la_date).unwrap_or(&empty_vec);
        assert_eq!(
            today_rhythms.len(),
            0,
            "Event at 11:30 PM LA time should count as done on LA date, not UTC date"
        );

        let next_day = la_date + chrono::Duration::days(1);
        let next_day_rhythms = schedule.get(&next_day).unwrap_or(&empty_vec);
        assert!(
            next_day_rhythms.iter().any(|r| r.id == daily.id),
            "Daily rhythm should appear on next day"
        );
    }

    #[test]
    fn timezone_boundary_daily_scheduling() {
        let mut manager = create_test_manager("America/Los_Angeles");

        let daily = create_test_rhythm("tz_daily2", Rhythm::Daily { at: TEST_AT });
        manager.set_rhythm(daily.clone()).unwrap();

        let la_date = NaiveDate::from_ymd_opt(2023, 8, 20).unwrap();
        let la_time = NaiveTime::from_hms_opt(23, 45, 0).unwrap();
        let la_datetime = la_date
            .and_time(la_time)
            .and_local_timezone(Los_Angeles)
            .earliest()
            .unwrap();

        manager.events.push(EventRecord {
            rhythm_id: daily.id,
            event_type: EventType::Done,
            when: la_datetime.with_timezone(&Utc),
            when_tz: "America/Los_Angeles".to_string(),
        });

        let start = la_date;
        let limit = la_date + chrono::Duration::days(3);
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        let empty_vec = Vec::new();
        let done_day_rhythms = schedule.get(&la_date).unwrap_or(&empty_vec);
        assert!(
            !done_day_rhythms.iter().any(|r| r.id == daily.id),
            "Daily should not appear on done day (Aug 20 LA time)"
        );

        let next_day = la_date + chrono::Duration::days(1);
        let next_day_rhythms = schedule.get(&next_day).unwrap_or(&empty_vec);
        assert!(
            next_day_rhythms.iter().any(|r| r.id == daily.id),
            "Daily should appear on Aug 21"
        );
    }

    #[test]
    fn timezone_boundary_defer_dates() {
        let mut manager = create_test_manager("America/Los_Angeles");

        let weekly = create_test_rhythm(
            "tz_weekly",
            Rhythm::WeekDaily {
                dotw: 6,
                at: TEST_AT,
                slider: Slider::new(1, 1),
            },
        );
        manager.set_rhythm(weekly.clone()).unwrap();

        let la_date = NaiveDate::from_ymd_opt(2023, 8, 20).unwrap();
        let la_time = NaiveTime::from_hms_opt(23, 55, 0).unwrap();
        let la_datetime = la_date
            .and_time(la_time)
            .and_local_timezone(Los_Angeles)
            .earliest()
            .unwrap();

        manager.events.push(EventRecord {
            rhythm_id: weekly.id,
            event_type: EventType::Defer,
            when: la_datetime.with_timezone(&Utc),
            when_tz: "America/Los_Angeles".to_string(),
        });

        let start = la_date;
        let limit = la_date + chrono::Duration::days(10);
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        let empty_vec = Vec::new();
        let deferred_day_rhythms = schedule.get(&la_date).unwrap_or(&empty_vec);
        assert!(
            !deferred_day_rhythms.iter().any(|r| r.id == weekly.id),
            "Weekly should not appear on deferred day (Aug 20 LA time, even though UTC is Aug 21)"
        );

        let day_before = la_date - chrono::Duration::days(1);
        let day_after = la_date + chrono::Duration::days(1);

        let appears_before = schedule
            .get(&day_before)
            .map(|rhythms| rhythms.iter().any(|r| r.id == weekly.id))
            .unwrap_or(false);
        let appears_after = schedule
            .get(&day_after)
            .map(|rhythms| rhythms.iter().any(|r| r.id == weekly.id))
            .unwrap_or(false);

        assert!(
            appears_before || appears_after,
            "Weekly should be rescheduled to adjacent day due to slider"
        );
    }

    #[test]
    fn timezone_early_morning_utc() {
        let mut manager = create_test_manager("America/Los_Angeles");

        let daily = create_test_rhythm("tz_early", Rhythm::Daily { at: TEST_AT });
        manager.set_rhythm(daily.clone()).unwrap();

        let la_date = NaiveDate::from_ymd_opt(2023, 8, 20).unwrap();
        let la_time = NaiveTime::from_hms_opt(2, 0, 0).unwrap();
        let la_datetime = la_date
            .and_time(la_time)
            .and_local_timezone(Los_Angeles)
            .earliest()
            .unwrap();

        manager.events.push(EventRecord {
            rhythm_id: daily.id,
            event_type: EventType::Done,
            when: la_datetime.with_timezone(&Utc),
            when_tz: "America/Los_Angeles".to_string(),
        });

        let start = la_date;
        let limit = la_date + chrono::Duration::days(2);
        let schedule_vec = manager.schedule(start, limit);
        let schedule = group_schedule_by_date(schedule_vec);

        let empty_vec = Vec::new();
        let done_day_rhythms = schedule.get(&la_date).unwrap_or(&empty_vec);
        assert!(
            !done_day_rhythms.iter().any(|r| r.id == daily.id),
            "Event at 2 AM LA time should be recognized as done on Aug 20 LA"
        );
    }
}
