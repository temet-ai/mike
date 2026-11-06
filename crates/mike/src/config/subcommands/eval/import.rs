use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, bail};

use super::FleetMirrorImportArgs;
use crate::config::{CliOverrides, load};
use crate::eval::labels::import::fleet_mirror::{ImportPlan, plan};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_labels::{ImportOutcome, ResolvedTurn};

async fn connect(cli: CliOverrides, execute: bool) -> Result<PersistHandle> {
    let config = load::load(cli).context("failed to resolve config for eval import")?;
    let Some(url) = config.database_url.as_deref() else {
        bail!("eval import requires a database_url (set it in /settings or MIKE_DATABASE_URL)");
    };
    let handle = if execute {
        PersistHandle::connect_existing(url).await
    } else {
        PersistHandle::connect_read_only(url).await
    };
    handle.context("eval import: could not connect to the database")
}

fn check_resolution(plan: &ImportPlan, resolved: &[ResolvedTurn]) -> Result<()> {
    let found: HashMap<i64, &ResolvedTurn> = resolved.iter().map(|r| (r.turn_id, r)).collect();
    let missing: BTreeSet<i64> = plan
        .turns
        .iter()
        .filter(|t| !found.contains_key(t))
        .copied()
        .collect();
    if !missing.is_empty() {
        bail!(
            "{} subject turn(s) are not in the capture DB: {missing:?}",
            missing.len()
        );
    }
    let open: BTreeSet<i64> = resolved
        .iter()
        .filter(|r| !r.closed)
        .map(|r| r.turn_id)
        .collect();
    if !open.is_empty() {
        bail!("{} subject turn(s) are still open: {open:?}", open.len());
    }
    let mismatched: BTreeSet<i64> = resolved
        .iter()
        .filter(|r| {
            plan.session_of_turn
                .get(&r.turn_id)
                .is_some_and(|asset| *asset != r.session_uuid)
        })
        .map(|r| r.turn_id)
        .collect();
    if !mismatched.is_empty() {
        bail!(
            "{} turn(s) belong to a different session than the assets say: {mismatched:?}",
            mismatched.len()
        );
    }
    Ok(())
}

pub(super) async fn fleet_mirror(args: FleetMirrorImportArgs, cli: CliOverrides) -> Result<()> {
    let plan = plan(&args.dir, &args.control_turns)?;
    let persist = connect(cli, args.execute).await?;
    let turns: Vec<i64> = plan.turns.iter().copied().collect();
    let resolved = persist.resolve_label_turns(&turns).await?;
    check_resolution(&plan, &resolved)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "manifest": plan.manifest,
            "counts": plan.counts,
            "turns_to_assign_dev": plan.turns.len(),
        }))?
    );
    if !args.execute {
        println!("Dry run only — pass --execute to write. No rows were written.");
        return Ok(());
    }
    let imported_by = std::env::var("USER").unwrap_or_else(|_| "mike".to_owned());
    match persist
        .import_fleet_mirror(&plan, &resolved, &imported_by)
        .await?
    {
        ImportOutcome::Written {
            batch_id,
            labels,
            splits,
            exclusions,
        } => println!(
            "imported batch {batch_id}: {labels} labels, {splits} new dev splits, {exclusions} population exclusions"
        ),
        ImportOutcome::AlreadyImported {
            batch_id,
            imported_at,
        } => {
            println!("already imported as batch {batch_id} at {imported_at}; nothing written");
        }
        ImportOutcome::TurnInHoldout(turns) => {
            bail!(
                "{} turn(s) are already in the holdout split: {turns:?}",
                turns.len()
            )
        }
    }
    Ok(())
}
