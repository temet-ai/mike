use std::collections::BTreeSet;

use super::definition::FleetDefinition;
use super::report::{CHECK_FAILED, CHECK_NOT_YET, CHECK_PASSED, check_at_commit};
use super::status::FleetStatus;
use crate::change_record::ChangeKind;
use crate::store::persistence::PersistHandle;

const CHECKS_READ: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    Accepted,
    Committed,
}

pub fn releases(ground: Option<Ground>, check: &str) -> bool {
    match ground {
        Some(Ground::Accepted) => check != CHECK_FAILED,
        Some(Ground::Committed) => check == CHECK_PASSED,
        None => false,
    }
}

pub async fn released(
    persist: &PersistHandle,
    def: &FleetDefinition,
    status: &FleetStatus,
) -> BTreeSet<String> {
    let accepted: BTreeSet<&str> = status
        .tasks
        .iter()
        .filter(|task| task.ready)
        .map(|task| task.id.as_str())
        .collect();
    let gate = Gate::read(persist, def).await;
    let mut out = BTreeSet::new();
    for id in producers(def) {
        let session = match status.run_id.as_deref() {
            Some(run) => super::definition::task_session(persist, run, &id).await,
            None => None,
        };
        let Some(session) = session else {
            if accepted.contains(id.as_str()) {
                out.insert(id);
            }
            continue;
        };
        let commit = super::ruling_proof::newest_commit(persist, session, &id).await;
        let ground = if accepted.contains(id.as_str()) {
            Some(Ground::Accepted)
        } else {
            commit.as_ref().map(|_| Ground::Committed)
        };
        if releases(
            ground,
            &gate.word_containing(&def.repo, commit.as_deref()).await,
        ) {
            out.insert(id);
        }
    }
    out
}

fn producers(def: &FleetDefinition) -> BTreeSet<String> {
    def.tasks
        .values()
        .flat_map(|task| task.after.iter().cloned())
        .collect()
}

struct Gate(Option<Vec<crate::change_record::ChangeEvent>>);

impl Gate {
    async fn read(persist: &PersistHandle, def: &FleetDefinition) -> Self {
        if def.check.command.trim().is_empty()
            || !crate::change_record::writers::change_record_enabled(persist).await
        {
            return Self(None);
        }
        Self(
            persist
                .latest_change_events(&def.repo, &[], &[ChangeKind::Check], CHECKS_READ)
                .await
                .ok(),
        )
    }

    fn word(&self, commit: Option<&str>) -> String {
        match (&self.0, commit) {
            (Some(events), Some(sha)) => check_at_commit(events, sha).to_string(),
            _ => "unknown".to_string(),
        }
    }

    async fn word_containing(&self, repo: &str, commit: Option<&str>) -> String {
        let word = self.word(commit);
        let (Some(events), Some(sha), true) = (&self.0, commit, word == CHECK_NOT_YET) else {
            return word;
        };
        for later in passed_shas(events) {
            if super::report_commit::ancestor(repo, sha, later).await {
                return CHECK_PASSED.to_string();
            }
        }
        word
    }
}

fn passed_shas(events: &[crate::change_record::ChangeEvent]) -> Vec<&str> {
    events
        .iter()
        .filter(|event| {
            event
                .detail
                .get("passed")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        })
        .filter_map(|event| {
            event.after_sha.as_deref().or_else(|| {
                event
                    .detail
                    .get("commit")
                    .and_then(serde_json::Value::as_str)
            })
        })
        .collect()
}

pub async fn commit_warning(
    persist: &PersistHandle,
    def: &FleetDefinition,
    previous: Option<&str>,
) -> Option<String> {
    let previous = previous?;
    let word = Gate::read(persist, def).await.word(Some(previous));
    commit_warning_for(&word, previous, &def.check.command)
}

pub fn commit_warning_for(word: &str, previous: &str, command: &str) -> Option<String> {
    (word == CHECK_FAILED).then(|| {
        format!(
            "the fleet's check `{command}` failed at your previous commit {previous} for this task; until it passes at a commit of yours, tasks waiting on yours do not start. Queue fleet_check now and fix what it names before you report"
        )
    })
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
