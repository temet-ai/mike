use chrono::NaiveDate;
use serde_json::{Value, json};
use uuid::Uuid;

use super::ledger::{Ledger, unpublished};
use super::model::{key_folder, key_project};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::skill_live::SkillLiveOutcome;
use crate::store::persistence::lessons::skill_review_writes::{InstallReport, ReportedInstall};
use crate::store::persistence::lessons_repo::LessonsPersist as _;

const MAX_OUTCOMES: usize = 500;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Published {
    pub sent: usize,
    pub error: Option<String>,
}

struct Waiting {
    key: String,
    outcome: SkillLiveOutcome,
}

fn parse_waiting(ledger: &Ledger) -> (Vec<Waiting>, Vec<String>) {
    let mut waiting = Vec::new();
    let mut problems = Vec::new();
    for (key, value) in unpublished(ledger) {
        match serde_json::from_value::<SkillLiveOutcome>(value) {
            Ok(outcome) => match outcome.inconsistency() {
                None => waiting.push(Waiting { key, outcome }),
                Some(problem) => problems.push(format!("{key}: {problem}")),
            },
            Err(error) => problems.push(format!("{key}: {error}")),
        }
    }
    (waiting, problems)
}

pub(super) async fn publish_outcomes(persist: &PersistHandle, ledger: &mut Ledger) -> Published {
    let (waiting, mut problems) = parse_waiting(ledger);
    let mut sent = 0;
    for chunk in waiting.chunks(MAX_OUTCOMES) {
        let outcomes: Vec<SkillLiveOutcome> = chunk.iter().map(|w| w.outcome.clone()).collect();
        if let Err(error) = persist
            .lessons()
            .upsert_skill_live_outcomes(&outcomes)
            .await
        {
            problems.push(error.to_string());
            break;
        }
        for item in chunk {
            if let Some(entry) = ledger.get_mut(&item.key) {
                entry.published = true;
            }
        }
        sent += chunk.len();
    }
    Published {
        sent,
        error: (!problems.is_empty()).then(|| problems.join("; ")),
    }
}

fn parse_day(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn installs(ledger: &Ledger) -> Vec<ReportedInstall> {
    ledger
        .iter()
        .filter_map(|(key, entry)| {
            let path = entry.path.clone().filter(|path| !path.trim().is_empty())?;
            let folder = key_folder(key);
            let retired_at = entry.retired_at.as_deref().and_then(parse_day);
            Some(ReportedInstall {
                project_uuid: Uuid::parse_str(key_project(key)).ok()?,
                folder: folder.to_owned(),
                path,
                draft_id: entry.draft_id,
                lesson_watermark: entry.lesson_watermark,
                first_written: parse_day(&entry.first_emitted)?,
                retired_at,
                retired_reason: retired_at.and(entry.retired_reason.clone()),
            })
            .filter(|install| !install.folder.trim().is_empty())
        })
        .collect()
}

pub(super) async fn report_installs(
    persist: &PersistHandle,
    machine: (Uuid, &str),
    ledger: &Ledger,
) -> Value {
    let rows = installs(ledger);
    let report = InstallReport {
        machine_uuid: machine.0,
        machine_name: machine.1,
        installs: &rows,
    };
    match persist.lessons().record_skill_installs(report).await {
        Ok(recorded) => json!({ "recorded": recorded }),
        Err(error) => json!({ "error": error.to_string() }),
    }
}

#[cfg(test)]
#[path = "publish_tests.rs"]
mod publish_tests;
