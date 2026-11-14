use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::io::Cursor;
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use super::{finish, prepare, start};
use crate::config::LaunchConfig;
use crate::launch::backend::fake::{FakeBackend, FakeCall};
use crate::launch::cli::ledger::Ledger;
use crate::launch::cli::test_support::{project_dir, runner};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{FleetRow, NewFleet};
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

async fn fleet() -> anyhow::Result<Option<(TestDbHandle, PersistHandle, FleetRow)>> {
    crate::launch::cli::fleet_settings::install();
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let row = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: format!("opening-{}", Uuid::new_v4()),
            project_path: "/fixture/opening".into(),
            definition: json!({}),
            missing: json!([]),
            created_by: None,
        })
        .await?;
    let ready = persist
        .fleet()
        .transition_fleet_state(&row, "ready", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("ready fixture"))?;
    let starting = persist
        .fleet()
        .transition_fleet_state(&ready, "starting", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("starting fixture"))?;
    let run = NewRun {
        run_id: format!("fleet-{}-12345678", row.slug),
        project_path: None,
        backend: "herdr".into(),
        workspace_ref: None,
        tab_ref: None,
    };
    let starting = persist
        .fleet()
        .initialize_fleet_run(&starting, &run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("initialized fixture"))?;
    let row = persist
        .fleet()
        .transition_fleet_state(&starting, "running", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("running fixture"))?;
    Ok(Some((db, persist, row)))
}

fn tokens(folder: &std::path::Path, row: &FleetRow) -> anyhow::Result<Vec<String>> {
    Ok(vec![
        folder.display().to_string(),
        "--run".into(),
        row.run_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!("fixture has no run"))?,
        "--seat".into(),
        "worker".into(),
        "--settings".into(),
        "{}".into(),
    ])
}

#[cfg_attr(test, tokio::test)]
pub async fn stopped_fleet_explicit_settings_cannot_bypass_actual_open_fence() -> anyhow::Result<()>
{
    let Some((db, persist, row)) = fleet().await? else {
        return Ok(());
    };
    let home = tempfile::tempdir()?;
    let folder = project_dir(home.path())?;
    let backend = FakeBackend::available();
    let config = LaunchConfig {
        channel_check_enabled: false,
        ..LaunchConfig::trusting()
    };
    let mut runner = runner(&backend, &config, home.path())?;
    runner.ledger = Ledger::new(Some(persist.clone()));
    let prepared = prepare(&runner, tokens(&folder, &row)?, &mut Cursor::new(""))
        .await
        .map_err(|failure| anyhow::anyhow!("{}", failure.error))?;
    persist
        .fleet()
        .transition_fleet_state(&row, "stopping", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("stop fixture"))?;
    let outcome = start(&runner, prepared).await;
    assert!(outcome.is_err());
    assert!(
        !backend
            .calls
            .lock()
            .await
            .iter()
            .any(|call| matches!(call, FakeCall::Open(_)))
    );
    db.cleanup().await
}

#[cfg_attr(test, tokio::test)]
pub async fn accepted_open_reservation_holds_stop_until_backend_open_is_durable()
-> anyhow::Result<()> {
    let Some((db, persist, row)) = fleet().await? else {
        return Ok(());
    };
    let home = tempfile::tempdir()?;
    let folder = project_dir(home.path())?;
    let backend = FakeBackend::available();
    let config = LaunchConfig {
        channel_check_enabled: false,
        ..LaunchConfig::trusting()
    };
    let mut runner = runner(&backend, &config, home.path())?;
    runner.ledger = Ledger::new(Some(persist.clone()));
    let prepared = prepare(&runner, tokens(&folder, &row)?, &mut Cursor::new(""))
        .await
        .map_err(|failure| anyhow::anyhow!("{}", failure.error))?;
    let started = start(&runner, prepared)
        .await
        .map_err(|failure| anyhow::anyhow!("{}", failure.error))?;
    assert_stop_waits_for_open(&runner, started, &persist, &row).await?;
    db.cleanup().await
}

async fn assert_stop_waits_for_open(
    runner: &crate::launch::cli::runner::Runner<'_>,
    started: super::Started,
    persist: &PersistHandle,
    row: &FleetRow,
) -> anyhow::Result<()> {
    let run = row
        .run_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("no fixture run"))?;
    let pending = persist.fleet().unsettled_fleet_launches(run).await?;
    assert_eq!(pending.len(), 1);
    assert!(!pending[0].opened);
    let stopping = persist
        .fleet()
        .transition_fleet_state(row, "stopping", None, None);
    tokio::pin!(stopping);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut stopping)
            .await
            .is_err()
    );
    let output = finish(runner, started)
        .await
        .map_err(|failure| anyhow::anyhow!("{}", failure.error))?;
    assert!(output.opened);
    let stopped = tokio::time::timeout(Duration::from_secs(2), stopping)
        .await??
        .ok_or_else(|| anyhow::anyhow!("stop lost its expected fixture state"))?;
    assert_eq!(stopped.state, "stopping");
    let remaining = persist.fleet().unsettled_fleet_launches(run).await?;
    assert_eq!(remaining.len(), 1);
    assert!(remaining[0].opened);
    assert!(
        persist
            .launch()
            .find_run(run)
            .await?
            .is_some_and(|run| run.closed_at.is_none())
    );
    Ok(())
}

#[cfg_attr(test, tokio::test)]
pub async fn backend_open_error_keeps_fleet_reservation_unresolved() -> anyhow::Result<()> {
    let Some((db, persist, row)) = fleet().await? else {
        return Ok(());
    };
    let home = tempfile::tempdir()?;
    let folder = project_dir(home.path())?;
    let backend = FakeBackend::available();
    backend
        .open_answers
        .lock()
        .await
        .push_back(Err(crate::launch::LaunchError::Failed(
            "backend timeout with unknown opening".into(),
        )));
    let config = LaunchConfig {
        channel_check_enabled: false,
        ..LaunchConfig::trusting()
    };
    let mut runner = runner(&backend, &config, home.path())?;
    runner.ledger = Ledger::new(Some(persist.clone()));
    let prepared = prepare(&runner, tokens(&folder, &row)?, &mut Cursor::new(""))
        .await
        .map_err(|failure| anyhow::anyhow!("{}", failure.error))?;
    let started = start(&runner, prepared)
        .await
        .map_err(|failure| anyhow::anyhow!("{}", failure.error))?;
    assert!(finish(&runner, started).await.is_err());
    let run = row
        .run_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("no fixture run"))?;
    let pending = persist.fleet().unsettled_fleet_launches(run).await?;
    assert_eq!(pending.len(), 1);
    assert!(!pending[0].opened);
    assert_eq!(pending[0].result, "pending");
    let stopping = persist
        .fleet()
        .transition_fleet_state(&row, "stopping", None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("stop fixture"))?;
    assert!(
        persist
            .fleet()
            .complete_fleet_run(&stopping, None)
            .await?
            .is_none()
    );
    db.cleanup().await
}
