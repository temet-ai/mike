use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use super::ledger::Ledger;
use crate::launch::fleet::{FleetDefinition, seat_settings};
use crate::launch::{LaunchError, SplitLaunchArgs};

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
pub(super) struct RecordedSettings {
    pub(super) run_id: Option<String>,
    pub(super) definition: Value,
}

fn requested(split: &SplitLaunchArgs) -> Option<(&str, &str)> {
    let run = split
        .run
        .as_deref()
        .filter(|run| run.starts_with("fleet-"))?;
    let kind = split.role.as_deref().or(split.seat.as_deref())?;
    let overridden = split
        .claude_args
        .iter()
        .any(|arg| arg == "--settings" || arg.starts_with("--settings="));
    (!overridden).then_some((run, kind))
}

fn slug_for_run(run: &str) -> Result<&str, LaunchError> {
    let parts = run
        .strip_prefix("fleet-")
        .and_then(|id| id.rsplit_once('-'));
    if let Some((slug, suffix)) = parts
        && !slug.is_empty()
        && slug != "."
        && slug != ".."
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && suffix.len() == 8
        && suffix.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Ok(slug);
    }
    Err(LaunchError::Usage(format!(
        "cannot inherit fleet seat settings: `{run}` is not a fleet run id; use the recorded \
         fleet-<slug>-<8 hex digits> run id or pass explicit --settings"
    )))
}

fn definition_for_run(
    run: &str,
    recorded: Option<RecordedSettings>,
) -> Result<FleetDefinition, LaunchError> {
    let row = recorded
        .filter(|row| row.run_id.as_deref() == Some(run))
        .ok_or_else(|| {
            LaunchError::Refused(format!(
                "cannot inherit fleet seat settings: no fleet is recorded with run `{run}`; \
                 check --run or pass explicit --settings"
            ))
        })?;
    FleetDefinition::from_value(&row.definition).map_err(|error| {
        LaunchError::Failed(format!(
            "cannot inherit seat settings for `{run}`: its fleet definition is invalid: {error}"
        ))
    })
}

pub(super) async fn inherit(
    ledger: &Ledger,
    split: &mut SplitLaunchArgs,
) -> Result<Option<FleetDefinition>, LaunchError> {
    let Some((run, kind)) = requested(split) else {
        return Ok(None);
    };
    let slug = slug_for_run(run)?;
    let recorded = tokio::time::timeout(LOOKUP_TIMEOUT, ledger.fleet_settings(slug))
        .await
        .map_err(|_| LaunchError::Timeout {
            command: format!("reading fleet seat settings for `{run}`"),
            after: LOOKUP_TIMEOUT,
        })?
        .map_err(|error| {
            LaunchError::Failed(format!(
                "cannot inherit seat settings for `{run}`: {error}; restore the fleet ledger \
                 or pass explicit --settings"
            ))
        })?;
    let def = definition_for_run(run, recorded)?;
    let settings = seat_settings::for_seat(&def, kind);
    split
        .claude_args
        .splice(0..0, ["--settings".to_string(), settings.to_string()]);
    Ok(Some(def))
}
