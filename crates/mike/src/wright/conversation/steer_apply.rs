use crate::store::persistence::web_repo::WebPersist as _;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::{Change, SteerApply, SteerError};
use crate::store::persistence::wright::conversation_steers::OperatorProblemUndo;
use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) const REMEDIES_NOT_AVAILABLE: &str = "known fixes are not in this build yet: pausing and resuming a remedy needs the remedies code (temet-ai/mike#74)";
pub(crate) const LIMITS_NOT_AVAILABLE: &str = "engineer limits are not in this build yet: changing one needs wright.engineer_limits (temet-ai/mike#73)";

pub(crate) struct Steers {
    pub(crate) persist: PersistHandle,
}

fn failed(e: &PersistError) -> SteerError {
    SteerError::Failed(format!("the database refused the steer: {e}"))
}

fn id_field(value: &Value, field: &str) -> Result<i64, SteerError> {
    value
        .get(field)
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .ok_or_else(|| SteerError::Failed(format!("the stored effect has no {field}: {value}")))
}

fn prior_healthy_at(prior: &Value) -> Option<DateTime<Utc>> {
    prior
        .get("healthy_at")
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|at| at.with_timezone(&Utc))
}

impl Steers {
    async fn read_open_problem(&self, check: &str, resource: &str) -> Result<Value, SteerError> {
        let web = self.persist.web();
        if !web
            .wright_check_exists(check)
            .await
            .map_err(|e| failed(&e))?
        {
            return Err(SteerError::Refused(format!(
                "open_problem names check `{check}`, and Wright has no check of that name"
            )));
        }
        let current = web
            .wright_operator_problem(check, resource)
            .await
            .map_err(|e| failed(&e))?;
        match current {
            Some(problem) if problem.get("state").and_then(Value::as_str) == Some("open") => {
                Err(SteerError::Refused(format!(
                    "the problem for check `{check}` and resource `{resource}` is already open: {problem}"
                )))
            }
            Some(problem) => Ok(problem),
            None => Ok(json!({"problem_id": null})),
        }
    }

    async fn read_skip(&self, id: i64) -> Result<Value, SteerError> {
        self.persist
            .web()
            .wright_problem_brief(id)
            .await
            .map_err(|e| failed(&e))?
            .ok_or_else(|| {
                SteerError::Refused(format!(
                    "skip_problem names problem {id}, and Wright has no problem with that id"
                ))
            })
    }

    async fn open_problem(
        &self,
        steer_id: i64,
        check: &str,
        resource: &str,
    ) -> Result<Value, SteerError> {
        let effect = self
            .persist
            .web()
            .open_wright_operator_problem(check, resource, steer_id)
            .await
            .map_err(|e| failed(&e))?
            .ok_or_else(|| SteerError::Refused(format!("Wright has no check named `{check}`")))?;
        if effect.get("opened_by_steer").and_then(Value::as_str)
            != Some(steer_id.to_string().as_str())
        {
            return Err(SteerError::Refused(format!(
                "the problem for check `{check}` and resource `{resource}` was opened by something else: {effect}"
            )));
        }
        Ok(effect)
    }

    async fn undo_open_problem(&self, prior: &Value, effect: &Value) -> Result<Value, SteerError> {
        let undo = OperatorProblemUndo {
            problem_id: id_field(effect, "problem_id")?,
            episode_id: id_field(effect, "episode_id")?,
            run_id: id_field(effect, "run_id")?,
            prior_healthy_at: prior_healthy_at(prior),
        };
        let undone = self
            .persist
            .web()
            .undo_wright_operator_problem(&undo)
            .await
            .map_err(|e| failed(&e))?;
        if !undone {
            return Err(SteerError::Conflict(format!(
                "problem {} changed after the steer opened it (a check pass saw it, healed it or another change touched it), so it is left as it is",
                undo.problem_id
            )));
        }
        Ok(prior.clone())
    }
}

#[async_trait]
impl SteerApply for Steers {
    async fn read(&self, change: &Change) -> Result<Value, SteerError> {
        match change {
            Change::PauseRemedy { .. } | Change::ResumeRemedy { .. } => {
                Err(SteerError::NotAvailable(REMEDIES_NOT_AVAILABLE.to_owned()))
            }
            Change::SetEngineerLimit { .. } => {
                Err(SteerError::NotAvailable(LIMITS_NOT_AVAILABLE.to_owned()))
            }
            Change::OpenProblem { check, resource } => {
                self.read_open_problem(check, resource).await
            }
            Change::SkipProblem { id, .. } => self.read_skip(*id).await,
        }
    }

    async fn apply(&self, steer_id: i64, change: &Change) -> Result<Value, SteerError> {
        match change {
            Change::PauseRemedy { .. } | Change::ResumeRemedy { .. } => {
                Err(SteerError::NotAvailable(REMEDIES_NOT_AVAILABLE.to_owned()))
            }
            Change::SetEngineerLimit { .. } => {
                Err(SteerError::NotAvailable(LIMITS_NOT_AVAILABLE.to_owned()))
            }
            Change::OpenProblem { check, resource } => {
                self.open_problem(steer_id, check, resource).await
            }
            Change::SkipProblem { id, until } => {
                Ok(json!({"problem_id": id, "skipped_until": until}))
            }
        }
    }

    async fn undo(
        &self,
        change: &Change,
        prior: &Value,
        effect: &Value,
    ) -> Result<Value, SteerError> {
        match change {
            Change::PauseRemedy { .. } | Change::ResumeRemedy { .. } => {
                Err(SteerError::NotAvailable(REMEDIES_NOT_AVAILABLE.to_owned()))
            }
            Change::SetEngineerLimit { .. } => {
                Err(SteerError::NotAvailable(LIMITS_NOT_AVAILABLE.to_owned()))
            }
            Change::OpenProblem { .. } => self.undo_open_problem(prior, effect).await,
            Change::SkipProblem { .. } => Ok(prior.clone()),
        }
    }
}
