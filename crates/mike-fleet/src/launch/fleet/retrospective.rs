use crate::store::persistence::fleet_repo::FleetPersist as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::checks::Missing;
use super::definition::FleetDefinition;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Retrospective {
    pub enabled: bool,
    pub model: String,
    pub write: Option<WriteAuthorization>,
    pub questions: Vec<String>,
    pub thread: String,
    pub issues_repo: Option<String>,
}

pub const DEFAULT_MODEL: &str = "sonnet";
pub const DEFAULT_THREAD: &str = "fleet-runs";
pub const DEFAULT_ISSUES_REPO: &str = "temet-ai/mike";
pub const DEFAULT_QUESTIONS: [&str; 4] = [
    "How did you split your effort between real work on your task and fleet machinery (briefs, mail, waiting, checks, reporting)? Give rough shares and the single biggest cost.",
    "Which instructions did you use, and which did you find noise or contradictory? Quote each one.",
    "What was missing from your brief, definition or tools that would have saved you time?",
    "Write the system prompt of about ten lines you would want for your task.",
];

impl Default for Retrospective {
    fn default() -> Self {
        Self {
            enabled: true,
            model: DEFAULT_MODEL.to_string(),
            write: None,
            questions: DEFAULT_QUESTIONS.iter().map(ToString::to_string).collect(),
            thread: DEFAULT_THREAD.to_string(),
            issues_repo: Some(DEFAULT_ISSUES_REPO.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteAuthorization {
    pub scope: String,
    pub branch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Pending,
    Launching,
    Running,
    Cleanup,
    Finished,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub host: String,
    pub source_run: String,
    pub run_id: String,
    pub session: Uuid,
    pub phase: Phase,
    pub attempts: u32,
    pub next_at: DateTime<Utc>,
    pub problem: Option<String>,
    pub source_definition: Value,
    pub roster: Value,
}

impl Job {
    pub fn new(source_run: &str, definition: &Value, roster: Value) -> Self {
        let session = Uuid::new_v4();
        let mut source_definition = definition.clone();
        if let Some(fields) = source_definition.as_object_mut() {
            fields.remove("retrospective_job");
        }
        Self {
            host: local_host(),
            source_run: source_run.to_string(),
            run_id: format!("retro-{source_run}-{session}"),
            session,
            phase: Phase::Pending,
            attempts: 0,
            next_at: Utc::now(),
            problem: None,
            source_definition,
            roster,
        }
    }

    pub fn retains_checkout(&self) -> bool {
        !matches!(self.phase, Phase::Finished | Phase::Failed)
    }
}

pub fn validate(def: &FleetDefinition) -> Vec<Missing> {
    let mut found = Vec::new();
    if let Err(err) =
        crate::store::persistence::work_threads::validate::slug(&def.retrospective.thread)
    {
        found.push(Missing::new("retrospective.thread", err.to_string()));
    }
    if def
        .retrospective
        .issues_repo
        .as_deref()
        .is_some_and(|repo| repo.trim().is_empty())
    {
        found.push(Missing::new(
            "retrospective.issues_repo",
            "name the repository as owner/name, or null to file no issues",
        ));
    }
    let Some(write) = &def.retrospective.write else {
        return found;
    };
    if write.scope.trim().is_empty() {
        found.push(Missing::new(
            "retrospective.write.scope",
            "explicit bounded authorization is required for retrospective mutations",
        ));
    }
    if write.branch.trim().is_empty() || write.branch.trim() == def.branch.trim() {
        found.push(Missing::new(
            "retrospective.write.branch",
            "name a separate retrospective branch, not the fleet's branch",
        ));
    }
    found
}

pub async fn save(
    persist: &PersistHandle,
    row: &FleetRow,
    job: &Job,
) -> Result<Option<FleetRow>, PersistError> {
    let mut definition = row.definition.clone();
    definition["retrospective_job"] = json!(job);
    let saved = persist
        .fleet()
        .save_fleet_definition(row.fleet_uuid, &definition, &row.missing, row.updated_at)
        .await?;
    Ok(saved.map(|updated_at| row.saved(definition, row.missing.clone(), updated_at)))
}

pub async fn stage(
    persist: &PersistHandle,
    row: &FleetRow,
    run: &str,
) -> Result<bool, PersistError> {
    for _ in 0..3 {
        let Some(fresh) = persist
            .fleet()
            .find_fleet(&row.fleet_uuid.to_string())
            .await?
        else {
            return Ok(false);
        };
        let def =
            FleetDefinition::from_value(&fresh.definition).map_err(PersistError::DataInvariant)?;
        if def
            .retrospective_job
            .as_ref()
            .is_some_and(|job| job.source_run == run)
        {
            return Ok(true);
        }
        if !def.retrospective.enabled {
            return Ok(false);
        }
        let roster = persist
            .fleet()
            .fleet_seat_rows(run)
            .await?
            .into_iter()
            .map(|seat| {
                json!({
                    "seat": seat.seat, "role": seat.role, "session": seat.session_uuid,
                    "model": seat.model, "context_tokens": seat.context_tokens,
                    "opened_at": seat.opened_at, "closed_at": seat.closed_at,
                })
            })
            .collect::<Vec<_>>();
        let job = Job::new(run, &fresh.definition, json!(roster));
        if save(persist, &fresh, &job).await?.is_some() {
            return Ok(true);
        }
    }
    Err(PersistError::DataInvariant(
        "the fleet changed while queuing its retrospective; close will retry".to_string(),
    ))
}

pub fn local_host() -> String {
    static HOST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HOST.get_or_init(|| {
        std::env::var("HOSTNAME")
            .ok()
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                let result = std::process::Command::new("hostname").output().ok()?;
                result
                    .status
                    .success()
                    .then(|| String::from_utf8_lossy(&result.stdout).trim().to_string())
            })
            .unwrap_or_default()
    })
    .clone()
}
