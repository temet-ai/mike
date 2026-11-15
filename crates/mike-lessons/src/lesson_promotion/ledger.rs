use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::model::{key_folder, key_project};
use crate::store::persistence::lessons::skill_live::{RetiredReason, SkillLiveOutcome};

pub(super) const MIN_LOADS_TO_RETIRE: i64 = 10;
pub(super) const UNLOADED_DAYS: i64 = 30;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(super) struct LedgerEntry {
    #[serde(default)]
    pub first_emitted: String,
    #[serde(default)]
    pub retired_at: Option<String>,
    #[serde(default)]
    pub lesson_watermark: i64,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub draft_id: Option<i64>,
    #[serde(default)]
    pub retired_reason: Option<String>,
    #[serde(default)]
    pub retired_ts: Option<String>,
    #[serde(default)]
    pub outcome: Option<serde_json::Value>,
    #[serde(default)]
    pub published: bool,
}

pub(super) type Ledger = BTreeMap<String, LedgerEntry>;
pub(super) type Emitted = BTreeMap<String, PathBuf>;

#[derive(Debug, Clone)]
pub(super) struct LiveRow {
    pub project_uuid: Uuid,
    pub folder: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub loads: i64,
    pub load_sessions: i64,
    pub loaded_turns: i64,
    pub judged_turns: i64,
    pub corrected_turns: i64,
    pub base_judged_turns: i64,
    pub base_corrected_turns: i64,
}

#[derive(Debug, Clone)]
pub(super) struct Live {
    pub row: LiveRow,
    pub first_shipped_on: NaiveDate,
}

pub(super) type Measured = BTreeMap<String, Live>;

pub(super) struct Shipped {
    pub project: Uuid,
    pub folder: String,
    pub since: NaiveDate,
}

fn is_folder_name(folder: &str) -> bool {
    !folder.is_empty()
        && folder.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn parse_day(day: &str) -> Option<NaiveDate> {
    if day.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
}

pub(super) fn shipped_skills(ledger: &Ledger, keys: &[String]) -> Vec<Shipped> {
    keys.iter()
        .filter_map(|key| {
            let project = Uuid::parse_str(key_project(key))
                .ok()
                .filter(|parsed| parsed.to_string() == key_project(key))?;
            let folder = key_folder(key);
            let since = parse_day(&ledger.get(key)?.first_emitted)?;
            is_folder_name(folder).then(|| Shipped {
                project,
                folder: folder.to_owned(),
                since,
            })
        })
        .collect()
}

pub(super) fn corrected_above_base(live: &LiveRow) -> bool {
    live.loads >= MIN_LOADS_TO_RETIRE
        && live.judged_turns > 0
        && live.base_judged_turns > 0
        && live.corrected_turns * live.base_judged_turns
            > live.base_corrected_turns * live.judged_turns
}

pub(super) fn mark_retired(entry: &mut LedgerEntry, today: NaiveDate, reason: &str) {
    entry.retired_at = Some(today.to_string());
    entry.retired_reason = Some(reason.to_owned());
    entry.retired_ts = Some(Utc::now().to_rfc3339());
}

pub(super) fn hold(ledger: &mut Ledger, key: &str, reason: &str, today: NaiveDate) {
    if let Some(entry) = ledger.get_mut(key)
        && entry.retired_at.is_none()
    {
        entry.retired_at = Some(today.to_string());
        entry.retired_reason = Some(reason.to_owned());
    }
}

fn drop_desired(desired: &mut BTreeMap<String, String>, path: &std::path::Path) {
    desired.remove(&path.display().to_string());
}

pub(super) fn retire_unloaded(
    desired: &mut BTreeMap<String, String>,
    ledger: &mut Ledger,
    loads: &BTreeMap<String, i64>,
    today: NaiveDate,
    emitted: &Emitted,
) -> Vec<String> {
    let retired: Vec<String> = emitted
        .keys()
        .filter(|key| {
            ledger.get(*key).is_some_and(|entry| {
                entry.retired_at.is_none()
                    && !loads.contains_key(*key)
                    && parse_day(&entry.first_emitted)
                        .is_some_and(|first| (today - first).num_days() >= UNLOADED_DAYS)
            })
        })
        .cloned()
        .collect();
    for key in &retired {
        if let Some(path) = emitted.get(key) {
            drop_desired(desired, path);
        }
        if let Some(entry) = ledger.get_mut(key) {
            mark_retired(entry, today, "unloaded");
        }
    }
    retired
}

pub(super) fn retire_corrected_above_base(
    desired: &mut BTreeMap<String, String>,
    ledger: &mut Ledger,
    measured: &Measured,
    today: NaiveDate,
    emitted: &Emitted,
) -> Vec<String> {
    let retired: Vec<String> = emitted
        .keys()
        .filter(|key| {
            ledger
                .get(*key)
                .is_some_and(|entry| entry.retired_at.is_none())
                && measured
                    .get(*key)
                    .is_some_and(|live| corrected_above_base(&live.row))
        })
        .cloned()
        .collect();
    for key in &retired {
        if let Some(path) = emitted.get(key) {
            drop_desired(desired, path);
        }
        if let Some(entry) = ledger.get_mut(key) {
            mark_retired(entry, today, "corrected_above_base");
        }
    }
    retired
}

fn retired_instant(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(moment) = DateTime::parse_from_rfc3339(value) {
        return Some(moment.with_timezone(&Utc));
    }
    parse_day(value)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|midnight| midnight.and_utc())
}

fn count(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub(super) fn outcome(key: &str, live: &Live, entry: &LedgerEntry) -> Option<SkillLiveOutcome> {
    let retired = entry.retired_ts.as_deref().or(entry.retired_at.as_deref());
    let retired_at = retired.and_then(retired_instant);
    let retired_reason = retired_at
        .and(entry.retired_reason.as_deref())
        .and_then(|reason| {
            serde_json::from_value::<RetiredReason>(serde_json::Value::String(reason.to_owned()))
                .ok()
        });
    let (retired_at, retired_reason) = match (retired_at, retired_reason) {
        (Some(at), Some(reason)) => (Some(at), Some(reason)),
        _ => (None, None),
    };
    let row = &live.row;
    Some(SkillLiveOutcome {
        project: Uuid::parse_str(key_project(key)).ok()?,
        folder: key_folder(key).to_owned(),
        first_shipped_on: live.first_shipped_on,
        window_start: row.window_start,
        window_end: row.window_end,
        loads: count(row.loads),
        load_sessions: count(row.load_sessions),
        loaded_turns: count(row.loaded_turns),
        judged_turns: count(row.judged_turns),
        corrected_turns: count(row.corrected_turns),
        base_judged_turns: count(row.base_judged_turns),
        base_corrected_turns: count(row.base_corrected_turns),
        retired_at,
        retired_reason,
    })
}

pub(super) fn record(ledger: &mut Ledger, measured: &Measured) {
    for (key, live) in measured {
        let Some(entry) = ledger.get_mut(key) else {
            continue;
        };
        let recorded = outcome(key, live, entry).and_then(|found| serde_json::to_value(found).ok());
        entry.outcome = recorded;
        entry.published = false;
    }
}

pub(super) fn unpublished(ledger: &Ledger) -> Vec<(String, serde_json::Value)> {
    ledger
        .iter()
        .filter(|(_, entry)| !entry.published)
        .filter_map(|(key, entry)| Some((key.clone(), entry.outcome.clone()?)))
        .collect()
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod ledger_tests;
