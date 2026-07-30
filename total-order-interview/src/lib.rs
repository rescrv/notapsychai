use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Inconsistent(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
            Self::Invalid(err) | Self::Inconsistent(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionLog {
    pub version: u32,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub timestamp: String,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    SessionCreated {
        title: String,
        notes: String,
        seed: u64,
    },
    FeaturesDefined {
        features: Vec<Feature>,
    },
    ComparisonAnswered {
        left: String,
        right: String,
        answer: Answer,
    },
    AnswerRetracted {
        event_id: String,
    },
    ReportGenerated {
        format: String,
        path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Feature {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    PreferLeft,
    PreferRight,
    Equal,
    RejectLeft,
    RejectRight,
    RejectBoth,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub values: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReportData {
    pub title: String,
    pub notes: String,
    pub seed: u64,
    pub features: Vec<Feature>,
    pub ranked_tiers: Vec<Vec<String>>,
    pub rejected: Vec<String>,
    pub unplaced: Vec<String>,
    pub coverage: Vec<FeatureCoverage>,
    pub driver_observations: Vec<DriverObservation>,
    pub exclusion_observations: Vec<ExclusionObservation>,
    pub skipped_pairs: Vec<PairObservation>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeatureCoverage {
    pub feature_id: String,
    pub feature_name: String,
    pub total_contexts: usize,
    pub ordering_effect: usize,
    pub no_effect: usize,
    pub rejected: usize,
    pub skipped: usize,
    pub untested: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriverObservation {
    pub feature_id: String,
    pub feature_name: String,
    pub true_preferred: usize,
    pub false_preferred: usize,
    pub no_effect: usize,
    pub mixed_or_unresolved: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExclusionObservation {
    pub feature_id: String,
    pub feature_name: String,
    pub true_rejected: usize,
    pub false_rejected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairObservation {
    pub left: String,
    pub right: String,
    pub answer: Answer,
}

#[derive(Debug, Clone)]
pub struct DerivedState {
    pub title: String,
    pub notes: String,
    pub seed: u64,
    pub features: Vec<Feature>,
    pub candidates: Vec<Candidate>,
    pub rejected: BTreeSet<String>,
    pub observed: BTreeSet<String>,
    pub active_comparisons: Vec<(String, String, Answer)>,
    pub order: OrderState,
}

#[derive(Debug, Clone)]
pub struct OrderState {
    parent: Vec<usize>,
    rank: Vec<u8>,
    edges: BTreeSet<(usize, usize)>,
    candidate_index: BTreeMap<String, usize>,
}

impl SessionLog {
    pub fn new(title: String, notes: String, features: Vec<String>, seed: u64) -> Result<Self> {
        validate_feature_names(&features)?;
        let features = features
            .into_iter()
            .enumerate()
            .map(|(idx, name)| Feature {
                id: format!("f{}", idx + 1),
                name,
            })
            .collect();
        let mut log = Self {
            version: 1,
            events: Vec::new(),
        };
        log.push(EventKind::SessionCreated { title, notes, seed });
        log.push(EventKind::FeaturesDefined { features });
        Ok(log)
    }

    pub fn push(&mut self, kind: EventKind) -> String {
        let id = format!("e{:06}", self.events.len() + 1);
        self.events.push(Event {
            id: id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            kind,
        });
        id
    }

    pub fn latest_active_answer_id(&self) -> Option<String> {
        let retracted = self.retracted_event_ids();
        self.events.iter().rev().find_map(|event| {
            if retracted.contains(&event.id) {
                return None;
            }
            matches!(event.kind, EventKind::ComparisonAnswered { .. }).then(|| event.id.clone())
        })
    }

    pub fn active_events(&self) -> Vec<Event> {
        let retracted = self.retracted_event_ids();
        self.events
            .iter()
            .filter(|event| !retracted.contains(&event.id))
            .cloned()
            .collect()
    }

    fn retracted_event_ids(&self) -> BTreeSet<String> {
        self.events
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::AnswerRetracted { event_id } => Some(event_id.clone()),
                _ => None,
            })
            .collect()
    }
}

pub fn load_session(path: &Path) -> Result<SessionLog> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn save_session_atomic(path: &Path, log: &SessionLog) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Invalid(format!("invalid session path: {}", path.display())))?;
    let tmp_path = parent.join(format!(".{file_name}.tmp"));
    let bytes = serde_json::to_vec_pretty(log)?;
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, path)?;
    Ok(())
}

pub fn derive_state(log: &SessionLog) -> Result<DerivedState> {
    let mut title = String::new();
    let mut notes = String::new();
    let mut seed = 0;
    let mut features = Vec::new();

    for event in log.active_events() {
        match event.kind {
            EventKind::SessionCreated {
                title: t,
                notes: n,
                seed: s,
            } => {
                title = t;
                notes = n;
                seed = s;
            }
            EventKind::FeaturesDefined { features: f } => {
                features = f;
            }
            _ => {}
        }
    }

    if features.is_empty() {
        return Err(Error::Invalid("session has no features".to_string()));
    }

    let candidates = generate_candidates(features.len())?;
    let mut rejected = BTreeSet::new();
    let mut observed = BTreeSet::new();
    let mut active_comparisons = Vec::new();

    for event in log.active_events() {
        if let EventKind::ComparisonAnswered {
            left,
            right,
            answer,
        } = event.kind
        {
            validate_candidate_id(&left, features.len())?;
            validate_candidate_id(&right, features.len())?;
            observed.insert(left.clone());
            observed.insert(right.clone());
            match answer {
                Answer::RejectLeft => {
                    rejected.insert(left.clone());
                }
                Answer::RejectRight => {
                    rejected.insert(right.clone());
                }
                Answer::RejectBoth => {
                    rejected.insert(left.clone());
                    rejected.insert(right.clone());
                }
                _ => {}
            }
            active_comparisons.push((left, right, answer));
        }
    }

    let mut order = OrderState::new(&candidates);
    for (left, right, answer) in &active_comparisons {
        if rejected.contains(left) || rejected.contains(right) {
            continue;
        }
        match answer {
            Answer::PreferLeft => order.add_order(left, right)?,
            Answer::PreferRight => order.add_order(right, left)?,
            Answer::Equal => order.add_equal(left, right)?,
            Answer::RejectLeft | Answer::RejectRight | Answer::RejectBoth | Answer::Skip => {}
        }
    }

    Ok(DerivedState {
        title,
        notes,
        seed,
        features,
        candidates,
        rejected,
        observed,
        active_comparisons,
        order,
    })
}

impl OrderState {
    fn new(candidates: &[Candidate]) -> Self {
        let candidate_index = candidates
            .iter()
            .enumerate()
            .map(|(idx, candidate)| (candidate.id.clone(), idx))
            .collect();
        Self {
            parent: (0..candidates.len()).collect(),
            rank: vec![0; candidates.len()],
            edges: BTreeSet::new(),
            candidate_index,
        }
    }

    pub fn add_equal(&mut self, left: &str, right: &str) -> Result<()> {
        let left = self.root_for_id(left)?;
        let right = self.root_for_id(right)?;
        if left == right {
            return Ok(());
        }
        if self.reachable(left, right) || self.reachable(right, left) {
            return Err(Error::Inconsistent(format!(
                "cannot merge ordered classes {left} and {right}"
            )));
        }
        let root = self.union_roots(left, right);
        let mut remapped = BTreeSet::new();
        for (from, to) in self.edges.iter().copied() {
            let from = self.find(from);
            let to = self.find(to);
            if from == to {
                return Err(Error::Inconsistent(format!(
                    "equality merge creates an order cycle at class {root}"
                )));
            }
            remapped.insert((from, to));
        }
        self.edges = remapped;
        Ok(())
    }

    pub fn add_order(&mut self, greater: &str, lesser: &str) -> Result<()> {
        let greater = self.root_for_id(greater)?;
        let lesser = self.root_for_id(lesser)?;
        if greater == lesser {
            return Err(Error::Inconsistent(format!(
                "cannot order candidate class {greater} above itself"
            )));
        }
        if self.reachable(lesser, greater) {
            return Err(Error::Inconsistent(format!(
                "ordering class {greater} above {lesser} creates a cycle"
            )));
        }
        self.edges.insert((greater, lesser));
        Ok(())
    }

    pub fn comparable_ids(&self, left: &str, right: &str) -> Result<bool> {
        let left = self.root_for_id(left)?;
        let right = self.root_for_id(right)?;
        Ok(left == right || self.reachable(left, right) || self.reachable(right, left))
    }

    pub fn equal_ids(&self, left: &str, right: &str) -> Result<bool> {
        Ok(self.root_for_id(left)? == self.root_for_id(right)?)
    }

    pub fn tiers(&self, candidate_ids: &[String]) -> Result<Vec<Vec<String>>> {
        let roots: BTreeSet<usize> = candidate_ids
            .iter()
            .map(|id| self.root_for_id(id))
            .collect::<Result<_>>()?;
        let mut by_root: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for id in candidate_ids {
            by_root
                .entry(self.root_for_id(id)?)
                .or_default()
                .push(id.clone());
        }
        let mut incoming: BTreeMap<usize, usize> = roots.iter().map(|root| (*root, 0)).collect();
        let mut outgoing: BTreeMap<usize, BTreeSet<usize>> =
            roots.iter().map(|root| (*root, BTreeSet::new())).collect();
        for (from, to) in self.edges.iter().copied() {
            let from = self.find(from);
            let to = self.find(to);
            if roots.contains(&from) && roots.contains(&to) && from != to {
                outgoing.entry(from).or_default().insert(to);
                *incoming.entry(to).or_default() += 1;
            }
        }
        let mut tiers = Vec::new();
        let mut ready: BTreeSet<usize> = incoming
            .iter()
            .filter_map(|(root, count)| (*count == 0).then_some(*root))
            .collect();
        let mut emitted = BTreeSet::new();
        while !ready.is_empty() {
            let layer: Vec<usize> = ready.iter().copied().collect();
            ready.clear();
            let mut tier = Vec::new();
            for root in &layer {
                emitted.insert(*root);
                if let Some(ids) = by_root.get(root) {
                    tier.extend(ids.clone());
                }
                if let Some(children) = outgoing.get(root) {
                    for child in children {
                        if let Some(count) = incoming.get_mut(child) {
                            *count -= 1;
                            if *count == 0 {
                                ready.insert(*child);
                            }
                        }
                    }
                }
            }
            tiers.push(tier);
        }
        if emitted.len() != roots.len() {
            return Err(Error::Inconsistent(
                "cannot create tiers from cyclic order".to_string(),
            ));
        }
        Ok(tiers)
    }

    fn root_for_id(&self, id: &str) -> Result<usize> {
        let idx = *self
            .candidate_index
            .get(id)
            .ok_or_else(|| Error::Invalid(format!("unknown candidate id: {id}")))?;
        Ok(self.find(idx))
    }

    fn find(&self, idx: usize) -> usize {
        let mut idx = idx;
        while self.parent[idx] != idx {
            idx = self.parent[idx];
        }
        idx
    }

    fn union_roots(&mut self, left: usize, right: usize) -> usize {
        let left = self.find(left);
        let right = self.find(right);
        if left == right {
            return left;
        }
        if self.rank[left] < self.rank[right] {
            self.parent[left] = right;
            right
        } else if self.rank[left] > self.rank[right] {
            self.parent[right] = left;
            left
        } else {
            self.parent[right] = left;
            self.rank[left] += 1;
            left
        }
    }

    fn reachable(&self, from: usize, to: usize) -> bool {
        let from = self.find(from);
        let to = self.find(to);
        if from == to {
            return true;
        }
        let mut queue = VecDeque::from([from]);
        let mut seen = BTreeSet::new();
        while let Some(node) = queue.pop_front() {
            if !seen.insert(node) {
                continue;
            }
            for (_, next) in self
                .edges
                .iter()
                .filter(|(edge_from, _)| self.find(*edge_from) == node)
            {
                let next = self.find(*next);
                if next == to {
                    return true;
                }
                queue.push_back(next);
            }
        }
        false
    }
}

pub fn generate_candidates(feature_count: usize) -> Result<Vec<Candidate>> {
    if feature_count == 0 {
        return Err(Error::Invalid(
            "at least one feature is required".to_string(),
        ));
    }
    if feature_count > 20 {
        return Err(Error::Invalid("too many features".to_string()));
    }
    let count = 1usize << feature_count;
    Ok((0..count)
        .map(|bits| {
            let values = (0..feature_count)
                .map(|idx| ((bits >> (feature_count - idx - 1)) & 1) == 1)
                .collect::<Vec<_>>();
            Candidate {
                id: candidate_id(&values),
                values,
            }
        })
        .collect())
}

pub fn candidate_id(values: &[bool]) -> String {
    let mut id = String::from("c");
    for value in values {
        id.push(if *value { '1' } else { '0' });
    }
    id
}

pub fn candidate_values(id: &str) -> Result<Vec<bool>> {
    if !id.starts_with('c') || id.len() == 1 {
        return Err(Error::Invalid(format!("invalid candidate id: {id}")));
    }
    id[1..]
        .chars()
        .map(|ch| match ch {
            '0' => Ok(false),
            '1' => Ok(true),
            _ => Err(Error::Invalid(format!("invalid candidate id: {id}"))),
        })
        .collect()
}

pub fn next_pair(state: &DerivedState) -> Result<Option<(String, String)>> {
    if let Some(pair) = next_coverage_pair(state)? {
        return Ok(Some(pair));
    }
    next_ordering_pair(state)
}

fn next_coverage_pair(state: &DerivedState) -> Result<Option<(String, String)>> {
    let feature_count = state.features.len();
    let tested = tested_matched_pairs(state)?;
    let last_pair = state
        .active_comparisons
        .last()
        .map(|(left, right, _)| normalized_pair(left, right));
    let mut best: Option<(usize, String, String)> = None;
    for feature_idx in 0..feature_count {
        let tested_for_feature = tested
            .iter()
            .filter(|((idx, _, _), answer)| *idx == feature_idx && **answer != Answer::Skip)
            .count();
        for candidate in &state.candidates {
            if candidate.values[feature_idx] {
                continue;
            }
            let mut other_values = candidate.values.clone();
            other_values[feature_idx] = true;
            let left = candidate.id.clone();
            let right = candidate_id(&other_values);
            if state.rejected.contains(&left) || state.rejected.contains(&right) {
                continue;
            }
            let key = matched_pair_key(feature_idx, &left, &right)?;
            if tested
                .get(&key)
                .is_some_and(|answer| *answer != Answer::Skip)
            {
                continue;
            }
            if last_pair.as_ref() == Some(&normalized_pair(&left, &right)) {
                continue;
            }
            match &best {
                None => best = Some((tested_for_feature, left, right)),
                Some((count, _, _)) if tested_for_feature < *count => {
                    best = Some((tested_for_feature, left, right));
                }
                _ => {}
            }
        }
    }
    Ok(best.map(|(_, left, right)| {
        orient_pair(state.seed, state.active_comparisons.len(), left, right)
    }))
}

fn next_ordering_pair(state: &DerivedState) -> Result<Option<(String, String)>> {
    let available: Vec<String> = state
        .candidates
        .iter()
        .filter(|candidate| !state.rejected.contains(&candidate.id))
        .map(|candidate| candidate.id.clone())
        .collect();
    let observed_available: Vec<String> = available
        .iter()
        .filter(|id| state.observed.contains(*id))
        .cloned()
        .collect();
    if let Some(unobserved) = available.iter().find(|id| !state.observed.contains(*id)) {
        if let Some(anchor) = observed_available.first() {
            return Ok(Some(orient_pair(
                state.seed,
                state.active_comparisons.len(),
                unobserved.clone(),
                anchor.clone(),
            )));
        }
        if let Some(second) = available.iter().find(|id| *id != unobserved) {
            return Ok(Some(orient_pair(
                state.seed,
                state.active_comparisons.len(),
                unobserved.clone(),
                second.clone(),
            )));
        }
    }
    for (idx, left) in observed_available.iter().enumerate() {
        for right in observed_available.iter().skip(idx + 1) {
            if !state.order.comparable_ids(left, right)? {
                return Ok(Some(orient_pair(
                    state.seed,
                    state.active_comparisons.len(),
                    left.clone(),
                    right.clone(),
                )));
            }
        }
    }
    Ok(None)
}

fn orient_pair(seed: u64, prompt_count: usize, left: String, right: String) -> (String, String) {
    let hash = stable_hash(seed, prompt_count as u64, &left, &right);
    if hash & 1 == 0 {
        (left, right)
    } else {
        (right, left)
    }
}

fn stable_hash(seed: u64, prompt_count: u64, left: &str, right: &str) -> u64 {
    let mut hash = seed ^ prompt_count.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for byte in left.bytes().chain(right.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

pub fn build_report_data(log: &SessionLog) -> Result<ReportData> {
    let state = derive_state(log)?;
    let available: Vec<String> = state
        .candidates
        .iter()
        .filter(|candidate| !state.rejected.contains(&candidate.id))
        .map(|candidate| candidate.id.clone())
        .collect();
    let observed_available: Vec<String> = available
        .iter()
        .filter(|id| state.observed.contains(*id))
        .cloned()
        .collect();
    let complete = all_pairwise_comparable(&state, &observed_available)?;
    let ranked_ids = if complete {
        observed_available.clone()
    } else {
        observed_available
            .iter()
            .filter(|id| {
                observed_available.iter().all(|other| {
                    id == &other || state.order.comparable_ids(id, other).unwrap_or(false)
                })
            })
            .cloned()
            .collect()
    };
    let ranked_set: BTreeSet<String> = ranked_ids.iter().cloned().collect();
    let mut unplaced: Vec<String> = available
        .into_iter()
        .filter(|id| !ranked_set.contains(id))
        .collect();
    unplaced.sort();
    let mut rejected: Vec<String> = state.rejected.iter().cloned().collect();
    rejected.sort();
    Ok(ReportData {
        title: state.title.clone(),
        notes: state.notes.clone(),
        seed: state.seed,
        features: state.features.clone(),
        ranked_tiers: state.order.tiers(&ranked_ids)?,
        rejected,
        unplaced,
        coverage: coverage(&state)?,
        driver_observations: driver_observations(&state)?,
        exclusion_observations: exclusion_observations(&state)?,
        skipped_pairs: state
            .active_comparisons
            .iter()
            .filter(|comparison| comparison.2 == Answer::Skip)
            .map(|(left, right, answer)| PairObservation {
                left: left.clone(),
                right: right.clone(),
                answer: *answer,
            })
            .collect(),
        events: log.active_events(),
    })
}

pub fn render_markdown_report(report: &ReportData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}", report.title);
    if !report.notes.trim().is_empty() {
        let _ = writeln!(out, "\n## Notes\n\n{}", report.notes.trim());
    }
    let _ = writeln!(out, "\n## Session Metadata\n");
    let _ = writeln!(out, "- Seed: `{}`", report.seed);
    let _ = writeln!(out, "- Active events: {}", report.events.len());

    let _ = writeln!(out, "\n## Features\n");
    for feature in &report.features {
        let _ = writeln!(out, "- `{}`: {}", feature.id, feature.name);
    }

    let _ = writeln!(out, "\n## Ranked Satisfying Candidates\n");
    if report.ranked_tiers.is_empty() {
        let _ = writeln!(out, "No candidates are fully placed yet.");
    } else {
        for (idx, tier) in report.ranked_tiers.iter().enumerate() {
            let _ = writeln!(out, "### Tier {}\n", idx + 1);
            for id in tier {
                let _ = writeln!(out, "- `{}` {}", id, format_candidate(id, &report.features));
            }
        }
    }

    let _ = writeln!(out, "\n## Rejected Candidates\n");
    write_candidate_list(&mut out, &report.rejected, &report.features);

    let _ = writeln!(out, "\n## Unplaced Candidates\n");
    write_candidate_list(&mut out, &report.unplaced, &report.features);

    let _ = writeln!(out, "\n## Matched-Pair Coverage\n");
    for item in &report.coverage {
        let _ = writeln!(
            out,
            "- {}: {} effect, {} no effect, {} rejected, {} skipped, {} untested of {} contexts",
            item.feature_name,
            item.ordering_effect,
            item.no_effect,
            item.rejected,
            item.skipped,
            item.untested,
            item.total_contexts
        );
    }

    let _ = writeln!(out, "\n## Variable-Driver Observations\n");
    for item in &report.driver_observations {
        let _ = writeln!(
            out,
            "- {}: true preferred {}, false preferred {}, no observed effect {}, mixed or unresolved {}.",
            item.feature_name,
            item.true_preferred,
            item.false_preferred,
            item.no_effect,
            item.mixed_or_unresolved
        );
    }

    let _ = writeln!(out, "\n## Exclusion Observations\n");
    for item in &report.exclusion_observations {
        let _ = writeln!(
            out,
            "- {}: true side rejected {}, false side rejected {}.",
            item.feature_name, item.true_rejected, item.false_rejected
        );
    }

    let _ = writeln!(out, "\n## Skipped Or Inconclusive Observations\n");
    if report.skipped_pairs.is_empty() {
        let _ = writeln!(out, "No skipped comparisons.");
    } else {
        for pair in &report.skipped_pairs {
            let _ = writeln!(out, "- `{}` vs `{}`", pair.left, pair.right);
        }
    }

    let _ = writeln!(out, "\n## Method Limits\n");
    let _ = writeln!(
        out,
        "- Observations are based only on user responses during this session."
    );
    let _ = writeln!(
        out,
        "- No diagnostic, causal, therapeutic, or psychological conclusions are made."
    );
    let _ = writeln!(
        out,
        "- No preference model, classifier, or prediction for unseen candidates is used."
    );
    let _ = writeln!(
        out,
        "- Untested or unplaced candidates are listed separately."
    );
    let _ = writeln!(
        out,
        "- Variable-driver claims require direct matched-pair evidence."
    );

    let _ = writeln!(out, "\n## Raw Event Appendix\n");
    for event in &report.events {
        let _ = writeln!(out, "- `{}` {} {:?}", event.id, event.timestamp, event.kind);
    }
    out
}

fn write_candidate_list(out: &mut String, ids: &[String], features: &[Feature]) {
    if ids.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        for id in ids {
            let _ = writeln!(out, "- `{}` {}", id, format_candidate(id, features));
        }
    }
}

pub fn format_candidate(id: &str, features: &[Feature]) -> String {
    match candidate_values(id) {
        Ok(values) => values
            .iter()
            .zip(features)
            .map(|(value, feature)| format!("{}={}", feature.name, if *value { "Y" } else { "N" }))
            .collect::<Vec<_>>()
            .join(", "),
        Err(_) => "<invalid candidate>".to_string(),
    }
}

pub fn format_pair_table(left: &str, right: &str, features: &[Feature]) -> Result<String> {
    let left_values = candidate_values(left)?;
    let right_values = candidate_values(right)?;
    let mut widths = vec![1usize; features.len()];
    for (idx, feature) in features.iter().enumerate() {
        widths[idx] = widths[idx].max(feature.name.len());
    }
    let label_width = 8usize;
    let mut out = String::new();
    let _ = write!(out, "{:label_width$}", "");
    for (idx, feature) in features.iter().enumerate() {
        let _ = write!(out, "  {:width$}", feature.name, width = widths[idx]);
    }
    out.push('\n');
    write_row(&mut out, "A", &left_values, &widths, label_width);
    write_row(&mut out, "B", &right_values, &widths, label_width);
    Ok(out)
}

fn write_row(out: &mut String, label: &str, values: &[bool], widths: &[usize], label_width: usize) {
    let _ = write!(out, "{label:label_width$}");
    for (idx, value) in values.iter().enumerate() {
        let value = if *value { "Y" } else { "N" };
        let _ = write!(out, "  {value:width$}", width = widths[idx]);
    }
    out.push('\n');
}

fn coverage(state: &DerivedState) -> Result<Vec<FeatureCoverage>> {
    let tested = tested_matched_pairs(state)?;
    let total_contexts = 1usize << (state.features.len() - 1);
    Ok(state
        .features
        .iter()
        .enumerate()
        .map(|(idx, feature)| {
            let mut ordering_effect = 0;
            let mut no_effect = 0;
            let mut rejected = 0;
            let mut skipped = 0;
            for ((feature_idx, _, _), answer) in &tested {
                if *feature_idx != idx {
                    continue;
                }
                match answer {
                    Answer::PreferLeft | Answer::PreferRight => ordering_effect += 1,
                    Answer::Equal => no_effect += 1,
                    Answer::RejectLeft | Answer::RejectRight | Answer::RejectBoth => rejected += 1,
                    Answer::Skip => skipped += 1,
                }
            }
            let tested_count = ordering_effect + no_effect + rejected + skipped;
            FeatureCoverage {
                feature_id: feature.id.clone(),
                feature_name: feature.name.clone(),
                total_contexts,
                ordering_effect,
                no_effect,
                rejected,
                skipped,
                untested: total_contexts.saturating_sub(tested_count),
            }
        })
        .collect())
}

fn driver_observations(state: &DerivedState) -> Result<Vec<DriverObservation>> {
    let mut observations = Vec::new();
    for (feature_idx, feature) in state.features.iter().enumerate() {
        let mut item = DriverObservation {
            feature_id: feature.id.clone(),
            feature_name: feature.name.clone(),
            true_preferred: 0,
            false_preferred: 0,
            no_effect: 0,
            mixed_or_unresolved: 0,
        };
        for (left, right, answer) in &state.active_comparisons {
            if hamming_distance(left, right)? != 1 || differing_feature(left, right)? != feature_idx
            {
                continue;
            }
            if state.rejected.contains(left) || state.rejected.contains(right) {
                continue;
            }
            let left_value = candidate_values(left)?[feature_idx];
            match answer {
                Answer::PreferLeft => {
                    if left_value {
                        item.true_preferred += 1;
                    } else {
                        item.false_preferred += 1;
                    }
                }
                Answer::PreferRight => {
                    if left_value {
                        item.false_preferred += 1;
                    } else {
                        item.true_preferred += 1;
                    }
                }
                Answer::Equal => item.no_effect += 1,
                Answer::Skip | Answer::RejectLeft | Answer::RejectRight | Answer::RejectBoth => {
                    item.mixed_or_unresolved += 1
                }
            }
        }
        observations.push(item);
    }
    Ok(observations)
}

fn exclusion_observations(state: &DerivedState) -> Result<Vec<ExclusionObservation>> {
    let mut observations = Vec::new();
    for (feature_idx, feature) in state.features.iter().enumerate() {
        let mut item = ExclusionObservation {
            feature_id: feature.id.clone(),
            feature_name: feature.name.clone(),
            true_rejected: 0,
            false_rejected: 0,
        };
        for (left, right, answer) in &state.active_comparisons {
            for rejected_id in rejected_ids_for_answer(left, right, *answer) {
                let value = candidate_values(&rejected_id)?[feature_idx];
                if value {
                    item.true_rejected += 1;
                } else {
                    item.false_rejected += 1;
                }
            }
        }
        observations.push(item);
    }
    Ok(observations)
}

fn rejected_ids_for_answer(left: &str, right: &str, answer: Answer) -> Vec<String> {
    match answer {
        Answer::RejectLeft => vec![left.to_string()],
        Answer::RejectRight => vec![right.to_string()],
        Answer::RejectBoth => vec![left.to_string(), right.to_string()],
        _ => Vec::new(),
    }
}

fn tested_matched_pairs(state: &DerivedState) -> Result<BTreeMap<(usize, String, String), Answer>> {
    let mut tested = BTreeMap::new();
    for (left, right, answer) in &state.active_comparisons {
        if hamming_distance(left, right)? == 1 {
            let idx = differing_feature(left, right)?;
            tested.insert(matched_pair_key(idx, left, right)?, *answer);
        }
    }
    Ok(tested)
}

fn matched_pair_key(
    feature_idx: usize,
    left: &str,
    right: &str,
) -> Result<(usize, String, String)> {
    let (left, right) = normalized_pair(left, right);
    Ok((feature_idx, left, right))
}

fn normalized_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn all_pairwise_comparable(state: &DerivedState, ids: &[String]) -> Result<bool> {
    for (idx, left) in ids.iter().enumerate() {
        for right in ids.iter().skip(idx + 1) {
            if !state.order.comparable_ids(left, right)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn hamming_distance(left: &str, right: &str) -> Result<usize> {
    let left = candidate_values(left)?;
    let right = candidate_values(right)?;
    if left.len() != right.len() {
        return Err(Error::Invalid("candidate lengths differ".to_string()));
    }
    Ok(left.iter().zip(right).filter(|(a, b)| **a != *b).count())
}

fn differing_feature(left: &str, right: &str) -> Result<usize> {
    let left = candidate_values(left)?;
    let right = candidate_values(right)?;
    left.iter()
        .zip(right)
        .enumerate()
        .find_map(|(idx, (a, b))| (a != &b).then_some(idx))
        .ok_or_else(|| Error::Invalid("candidates do not differ".to_string()))
}

fn validate_feature_names(features: &[String]) -> Result<()> {
    if features.is_empty() {
        return Err(Error::Invalid(
            "at least one feature is required".to_string(),
        ));
    }
    if features.len() > 12 {
        return Err(Error::Invalid(
            "v1 supports at most 12 features by default".to_string(),
        ));
    }
    let mut seen = BTreeSet::new();
    for feature in features {
        if feature.trim().is_empty() {
            return Err(Error::Invalid("feature names cannot be empty".to_string()));
        }
        if !seen.insert(feature.to_lowercase()) {
            return Err(Error::Invalid(format!("duplicate feature name: {feature}")));
        }
    }
    Ok(())
}

fn validate_candidate_id(id: &str, feature_count: usize) -> Result<()> {
    let values = candidate_values(id)?;
    if values.len() != feature_count {
        return Err(Error::Invalid(format!(
            "candidate {id} has {} values, expected {feature_count}",
            values.len()
        )));
    }
    Ok(())
}

pub fn seed_from_clock() -> u64 {
    Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64
}

pub fn write_text(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text)?;
    Ok(())
}

pub fn append_report_events(log: &mut SessionLog, md: Option<&Path>, json: Option<&Path>) {
    if let Some(path) = md {
        log.push(EventKind::ReportGenerated {
            format: "markdown".to_string(),
            path: path.display().to_string(),
        });
    }
    if let Some(path) = json {
        log.push(EventKind::ReportGenerated {
            format: "json".to_string(),
            path: path.display().to_string(),
        });
    }
}

pub fn parse_report_args(args: &[String]) -> Result<(Option<PathBuf>, Option<PathBuf>)> {
    let mut md = None;
    let mut json = None;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--md" => {
                idx += 1;
                let path = args
                    .get(idx)
                    .ok_or_else(|| Error::Invalid("--md requires a path".to_string()))?;
                md = Some(PathBuf::from(path));
            }
            "--json" => {
                idx += 1;
                let path = args
                    .get(idx)
                    .ok_or_else(|| Error::Invalid("--json requires a path".to_string()))?;
                json = Some(PathBuf::from(path));
            }
            other => return Err(Error::Invalid(format!("unknown report argument: {other}"))),
        }
        idx += 1;
    }
    if md.is_none() && json.is_none() {
        return Err(Error::Invalid(
            "report requires --md PATH, --json PATH, or both".to_string(),
        ));
    }
    Ok((md, json))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_with_features(names: &[&str]) -> SessionLog {
        SessionLog::new(
            "test".to_string(),
            "".to_string(),
            names.iter().map(|name| name.to_string()).collect(),
            7,
        )
        .unwrap()
    }

    fn answer(log: &mut SessionLog, left: &str, right: &str, answer: Answer) -> String {
        log.push(EventKind::ComparisonAnswered {
            left: left.to_string(),
            right: right.to_string(),
            answer,
        })
    }

    #[test]
    fn equality_transitivity_places_three_candidates_in_one_tier() {
        let mut log = log_with_features(&["A", "B"]);
        answer(&mut log, "c00", "c01", Answer::Equal);
        answer(&mut log, "c01", "c11", Answer::Equal);

        let report = build_report_data(&log).unwrap();

        assert_eq!(
            ReportData {
                title: "test".to_string(),
                notes: "".to_string(),
                seed: 7,
                features: vec![
                    Feature {
                        id: "f1".to_string(),
                        name: "A".to_string()
                    },
                    Feature {
                        id: "f2".to_string(),
                        name: "B".to_string()
                    }
                ],
                ranked_tiers: vec![vec![
                    "c00".to_string(),
                    "c01".to_string(),
                    "c11".to_string()
                ]],
                rejected: vec![],
                unplaced: vec!["c10".to_string()],
                coverage: vec![
                    FeatureCoverage {
                        feature_id: "f1".to_string(),
                        feature_name: "A".to_string(),
                        total_contexts: 2,
                        ordering_effect: 0,
                        no_effect: 1,
                        rejected: 0,
                        skipped: 0,
                        untested: 1,
                    },
                    FeatureCoverage {
                        feature_id: "f2".to_string(),
                        feature_name: "B".to_string(),
                        total_contexts: 2,
                        ordering_effect: 0,
                        no_effect: 1,
                        rejected: 0,
                        skipped: 0,
                        untested: 1,
                    },
                ],
                driver_observations: vec![
                    DriverObservation {
                        feature_id: "f1".to_string(),
                        feature_name: "A".to_string(),
                        true_preferred: 0,
                        false_preferred: 0,
                        no_effect: 1,
                        mixed_or_unresolved: 0,
                    },
                    DriverObservation {
                        feature_id: "f2".to_string(),
                        feature_name: "B".to_string(),
                        true_preferred: 0,
                        false_preferred: 0,
                        no_effect: 1,
                        mixed_or_unresolved: 0,
                    },
                ],
                exclusion_observations: vec![
                    ExclusionObservation {
                        feature_id: "f1".to_string(),
                        feature_name: "A".to_string(),
                        true_rejected: 0,
                        false_rejected: 0,
                    },
                    ExclusionObservation {
                        feature_id: "f2".to_string(),
                        feature_name: "B".to_string(),
                        true_rejected: 0,
                        false_rejected: 0,
                    },
                ],
                skipped_pairs: vec![],
                events: log.active_events(),
            },
            report
        );
    }

    #[test]
    fn order_transitivity_makes_endpoints_comparable() {
        let mut log = log_with_features(&["A", "B"]);
        answer(&mut log, "c00", "c01", Answer::PreferLeft);
        answer(&mut log, "c01", "c11", Answer::PreferLeft);

        let state = derive_state(&log).unwrap();

        assert!(state.order.comparable_ids("c00", "c11").unwrap());
    }

    #[test]
    fn rejected_candidates_are_excluded_from_ordering_report() {
        let mut log = log_with_features(&["A"]);
        answer(&mut log, "c0", "c1", Answer::RejectRight);

        let report = build_report_data(&log).unwrap();

        assert_eq!(vec!["c1".to_string()], report.rejected);
        assert_eq!(vec!["c0".to_string()], report.ranked_tiers.concat());
        assert_eq!(Vec::<String>::new(), report.unplaced);
    }

    #[test]
    fn cycle_is_detected() {
        let mut log = log_with_features(&["A", "B"]);
        answer(&mut log, "c00", "c01", Answer::PreferLeft);
        answer(&mut log, "c01", "c11", Answer::PreferLeft);
        answer(&mut log, "c11", "c00", Answer::PreferLeft);

        let err = derive_state(&log).unwrap_err().to_string();

        assert!(err.contains("cycle"));
    }

    #[test]
    fn equality_merge_detects_existing_order_conflict() {
        let mut log = log_with_features(&["A"]);
        answer(&mut log, "c0", "c1", Answer::PreferLeft);
        answer(&mut log, "c0", "c1", Answer::Equal);

        let err = derive_state(&log).unwrap_err().to_string();

        assert!(err.contains("cannot merge ordered classes"));
    }

    #[test]
    fn matched_pair_attribution_ignores_multi_variable_comparisons() {
        let mut log = log_with_features(&["A", "B"]);
        answer(&mut log, "c00", "c11", Answer::PreferRight);

        let report = build_report_data(&log).unwrap();

        assert_eq!(
            vec![
                DriverObservation {
                    feature_id: "f1".to_string(),
                    feature_name: "A".to_string(),
                    true_preferred: 0,
                    false_preferred: 0,
                    no_effect: 0,
                    mixed_or_unresolved: 0,
                },
                DriverObservation {
                    feature_id: "f2".to_string(),
                    feature_name: "B".to_string(),
                    true_preferred: 0,
                    false_preferred: 0,
                    no_effect: 0,
                    mixed_or_unresolved: 0,
                },
            ],
            report.driver_observations
        );
    }

    #[test]
    fn no_preference_in_matched_pair_counts_as_observed_no_effect() {
        let mut log = log_with_features(&["A"]);
        answer(&mut log, "c0", "c1", Answer::Equal);

        let report = build_report_data(&log).unwrap();

        assert_eq!(
            vec![DriverObservation {
                feature_id: "f1".to_string(),
                feature_name: "A".to_string(),
                true_preferred: 0,
                false_preferred: 0,
                no_effect: 1,
                mixed_or_unresolved: 0,
            }],
            report.driver_observations
        );
    }

    #[test]
    fn skipped_pairs_create_no_ordering_or_attribution_evidence() {
        let mut log = log_with_features(&["A"]);
        answer(&mut log, "c0", "c1", Answer::Skip);

        let report = build_report_data(&log).unwrap();

        assert_eq!(Vec::<Vec<String>>::new(), report.ranked_tiers);
        assert_eq!(
            vec![PairObservation {
                left: "c0".to_string(),
                right: "c1".to_string(),
                answer: Answer::Skip,
            }],
            report.skipped_pairs
        );
        assert_eq!(
            vec![DriverObservation {
                feature_id: "f1".to_string(),
                feature_name: "A".to_string(),
                true_preferred: 0,
                false_preferred: 0,
                no_effect: 0,
                mixed_or_unresolved: 1,
            }],
            report.driver_observations
        );
    }

    #[test]
    fn append_only_retraction_removes_answer_from_derived_state() {
        let mut log = log_with_features(&["A"]);
        let event_id = answer(&mut log, "c0", "c1", Answer::PreferLeft);
        log.push(EventKind::AnswerRetracted { event_id });

        let report = build_report_data(&log).unwrap();

        assert_eq!(Vec::<Vec<String>>::new(), report.ranked_tiers);
        assert_eq!(vec!["c0".to_string(), "c1".to_string()], report.unplaced);
        assert_eq!(
            vec![FeatureCoverage {
                feature_id: "f1".to_string(),
                feature_name: "A".to_string(),
                total_contexts: 1,
                ordering_effect: 0,
                no_effect: 0,
                rejected: 0,
                skipped: 0,
                untested: 1,
            }],
            report.coverage
        );
    }
}
