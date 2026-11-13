use crate::store::persistence::eval_repo::EvalPersist as _;
use anyhow::{Context, Result};

use super::args_runs::EvalShowArgs;
use crate::config::CliOverrides;
use crate::store::persistence::eval_runs::RunItemsFilter;

const ITEM_PAGE: i64 = 500;

async fn print_items(
    persist: &crate::store::persistence::PersistHandle,
    run_id: i64,
) -> Result<()> {
    let mut offset = 0;
    loop {
        let filter = RunItemsFilter {
            include_verdict: true,
            limit: ITEM_PAGE,
            offset,
            ..RunItemsFilter::default()
        };
        let (items, total) = persist.eval().list_eval_run_items(run_id, &filter).await?;
        for item in &items {
            println!("{}", serde_json::to_string(item)?);
        }
        offset += ITEM_PAGE;
        if items.is_empty() || offset >= total {
            return Ok(());
        }
    }
}

pub(super) async fn show(args: EvalShowArgs, cli: CliOverrides) -> Result<i32> {
    let persist = super::connect(cli, false).await?;
    if args.gate {
        let gate = persist
            .eval()
            .get_eval_gate(args.id)
            .await?
            .with_context(|| format!("no eval gate {}", args.id))?;
        println!("{}", serde_json::to_string_pretty(&gate)?);
        return Ok(0);
    }
    let run = persist
        .eval()
        .get_eval_run(args.id)
        .await?
        .with_context(|| format!("no eval run {}", args.id))?;
    if args.json {
        println!("{}", serde_json::to_string(&run)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&run)?);
    }
    if args.items {
        print_items(&persist, args.id).await?;
    }
    Ok(0)
}
