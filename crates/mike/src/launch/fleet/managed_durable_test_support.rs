use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::super::check::Runner;
use super::super::definition::FleetDefinition;
use super::super::gate::Target;
use super::super::managed_command::{self, Queued};
use super::super::plan::FleetFacts;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::NewRun;

pub(in crate::launch::fleet) async fn running_fleet(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
) -> anyhow::Result<Uuid> {
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: run.into(),
            project_path: Some(def.repo.clone()),
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let fleet = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: format!("managed-test-{}", Uuid::new_v4()),
            project_path: def.repo.clone(),
            definition: serde_json::to_value(def)?,
            missing: serde_json::json!([]),
            created_by: None,
        })
        .await?;
    crate::store::persistence::test_support::seed_fleet_running(persist, fleet.fleet_uuid, run)
        .await?;
    Ok(fleet.fleet_uuid)
}

pub(in crate::launch::fleet) async fn startup_gate(
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    target: &Target,
    tracker: &TaskTracker,
) -> anyhow::Result<(Option<i32>, String, bool)> {
    let queued =
        match managed_command::begin_baseline(persist, def, facts, &target.commit, tracker).await {
            Ok(queued) => queued,
            Err(why) => return Ok((None, why, false)),
        };
    registered_gate(persist, def, target, queued, tracker).await
}

pub(in crate::launch::fleet) async fn durable_gate(
    runner: &Runner<'_>,
    def: &FleetDefinition,
    target: &Target,
) -> anyhow::Result<(Option<i32>, String, bool)> {
    let queued = managed_command::begin_gate(runner, def, "fixture", target)
        .await
        .map_err(anyhow::Error::msg)?;
    registered_gate(runner.persist, def, target, queued, runner.tracker).await
}

async fn registered_gate(
    persist: &PersistHandle,
    def: &FleetDefinition,
    target: &Target,
    queued: Queued,
    tracker: &TaskTracker,
) -> anyhow::Result<(Option<i32>, String, bool)> {
    let owner = queued.context.owner.clone();
    let ledger = queued.context.ledger.clone();
    let run = queued
        .context
        .authority
        .durable()
        .map_err(anyhow::Error::msg)?
        .run;
    let (mut code, mut output, mut verified) = managed_command::scope(
        queued.context,
        super::super::gate::execute_verified(def, target, &run),
    )
    .await;
    if managed_command::normalize_cancellation(&owner, &mut code, &mut output) {
        verified = false;
    }
    let captured = super::super::check_run::completion::Captured {
        id: queued.id,
        exit_code: code,
        output: output.clone(),
        state_after: if owner.cancelled().is_some() {
            "cancelled:unverified".into()
        } else if verified {
            target.commit.clone()
        } else {
            "unverified-source".into()
        },
    };
    match super::super::check_run::completion::record(
        persist, tracker, &ledger, &def.repo, captured,
    )
    .await
    {
        Ok(_) => queued.registration.receipt_recorded(),
        Err(why) => {
            queued.registration.receipt_blocked(why.clone());
            anyhow::bail!(why);
        }
    }
    Ok((code, output, verified))
}
