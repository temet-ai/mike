use std::time::Duration;

use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::sync::Arc;

use super::ledger::Ledger;
use crate::launch::fleet::{FleetDefinition, seat_settings};
use crate::launch::{LaunchError, SplitLaunchArgs};

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

pub use crate::launch::seat::RecordedSettings;

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

pub async fn inherit(
    ledger: &Ledger,
    split: &mut SplitLaunchArgs,
) -> Result<Option<FleetDefinition>, LaunchError> {
    let Some((run, kind)) = requested(split) else {
        return Ok(None);
    };
    let slug = slug_for_run(run)?;
    let recorded = tokio::time::timeout(LOOKUP_TIMEOUT, recorded_settings(ledger, slug))
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

async fn recorded_settings(ledger: &Ledger, key: &str) -> Result<Option<RecordedSettings>, String> {
    if let Some(handle) = ledger.database() {
        return handle
            .fleet()
            .find_fleet(key)
            .await
            .map(|row| {
                row.map(|row| RecordedSettings {
                    run_id: row.run_id,
                    definition: row.definition,
                })
            })
            .map_err(|e| e.to_string());
    }
    match ledger.proxy_ledger() {
        Some(proxy) => proxy.fleet_settings(key).await,
        None => Err("no database and no proxy to record through".to_string()),
    }
}

pub struct FleetSeatHook;

impl crate::launch::seat::SeatLaunchHook for FleetSeatHook {
    fn before_open<'a>(
        &'a self,
        ledger: &'a Ledger,
        split: &'a mut SplitLaunchArgs,
    ) -> crate::launch::seat::SeatFuture<'a> {
        Box::pin(async move {
            if let Some(def) = inherit(ledger, split).await? {
                seat_settings::prepare(&def).map_err(LaunchError::Failed)?;
            }
            Ok(())
        })
    }

    fn admit<'a>(
        &'a self,
        ledger: &'a Ledger,
        run: &'a str,
    ) -> crate::launch::seat::AdmitFuture<'a> {
        Box::pin(async move {
            let admission = super::open::fleet_admission::admit(ledger, run).await?;
            Ok(admission.map(|a| Box::new(a) as Box<dyn crate::launch::seat::SeatAdmission>))
        })
    }
}

pub fn install() {
    crate::launch::seat::install_hook(Arc::new(FleetSeatHook));
}
