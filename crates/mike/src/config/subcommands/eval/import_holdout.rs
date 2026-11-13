use crate::store::persistence::eval_repo::EvalPersist as _;
use std::collections::BTreeSet;

use anyhow::{Result, bail};

use super::HoldoutImportArgs;
use crate::config::CliOverrides;
use crate::eval::labels::import::holdout::plan;
use crate::store::persistence::eval_labels::{HoldoutOutcome, SplitConflictPolicy};

pub(super) async fn holdout(args: HoldoutImportArgs, cli: CliOverrides) -> Result<()> {
    let plan = plan(&args.dir, &args.batch).map_err(anyhow::Error::msg)?;
    let persist = super::connect(cli, args.execute).await?;
    let turns: Vec<i64> = plan.turns.iter().copied().collect();
    let resolved = persist.eval().resolve_label_turns(&turns).await?;
    let found: BTreeSet<i64> = resolved.iter().map(|r| r.turn_id).collect();
    let missing: Vec<i64> = plan
        .turns
        .iter()
        .filter(|t| !found.contains(t))
        .copied()
        .collect();
    if !missing.is_empty() {
        bail!(
            "{} holdout turn(s) are not in the capture DB: {missing:?}",
            missing.len()
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"batch": plan.batch, "turns": plan.turns.len(), "counts": plan.counts})
        )?
    );
    if !args.execute {
        println!("Dry run only — pass --execute to write. No rows were written.");
        return Ok(());
    }
    let imported_by = std::env::var("USER").unwrap_or_else(|_| "mike".to_owned());
    let policy = conflict_policy(&args);
    match persist
        .eval()
        .import_holdout(&plan, &resolved, &imported_by, policy)
        .await?
    {
        HoldoutOutcome::Written {
            batch_id,
            labels,
            splits,
            dropped,
        } => {
            println!(
                "imported holdout batch {batch_id}: {labels} labels, {splits} new holdout splits"
            );
            if !dropped.is_empty() {
                println!(
                    "dropped {} turn(s) already in the dev split: {dropped:?}",
                    dropped.len()
                );
            }
        }
        HoldoutOutcome::AlreadyImported {
            batch_id,
            imported_at,
        } => {
            println!("already imported as batch {batch_id} at {imported_at}; nothing written");
        }
        HoldoutOutcome::DevSplitConflict(turns) => bail!(
            "{} turn(s) are already in the dev split: {turns:?}; pass --drop-split-conflicts to drop them, or --resplit-unused-dev to move them when no eval run used their dev labels",
            turns.len()
        ),
        HoldoutOutcome::DevLabelsInUse(turns) => bail!(
            "{} dev-split turn(s) have labels an eval run used, so they cannot become holdout: {turns:?}; nothing was written",
            turns.len()
        ),
    }
    Ok(())
}

fn conflict_policy(args: &HoldoutImportArgs) -> SplitConflictPolicy {
    if args.resplit_unused_dev {
        SplitConflictPolicy::ResplitUnusedDev
    } else if args.drop_split_conflicts {
        SplitConflictPolicy::Drop
    } else {
        SplitConflictPolicy::Refuse
    }
}
