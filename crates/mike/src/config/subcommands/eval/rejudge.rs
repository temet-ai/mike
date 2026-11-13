use crate::store::persistence::eval_repo::EvalPersist as _;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};

use super::args_runs::EvalRejudgeArgs;
use crate::config::CliOverrides;
use crate::eval::rejudge::{
    RejudgePlan, cancel_active, plan_rejudge, plan_session_rejudge, start_rejudge,
    start_session_rejudge,
};

fn parse(label: &str, value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .with_context(|| format!("--{label} {value:?} is not an RFC 3339 timestamp"))
}

fn print_plan(plan: &RejudgePlan) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(plan)?);
    println!(
        "bundle {} (gate {}): {} eligible turns lack a verdict, {} ineligible turns are re-projected to unknown; about {} calls{} over {} day(s) at {} a day",
        plan.window.bundle_id,
        plan.gate_id,
        plan.progress.remaining,
        plan.progress.ineligible_total,
        plan.estimated_calls,
        plan.estimated_usd
            .map_or_else(String::new, |usd| format!(" (${usd:.2})")),
        plan.days_at_cap,
        plan.daily_cap
    );
    println!(
        "judge budget at the ${:.2} ceiling: slice ${:.2}, shared pool ${:.2}",
        plan.budget.ceiling_usd, plan.budget.slice_usd, plan.budget.pool_usd
    );
    Ok(())
}

async fn status(persist: &crate::store::persistence::PersistHandle) -> Result<()> {
    let runs = persist.eval().list_rejudge_runs().await?;
    let Some(run) = runs.into_iter().next() else {
        println!("no re-judge run has been started");
        return Ok(());
    };
    let progress = persist.eval().rejudge_progress(&run.window()).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "run": run, "progress": progress }))?
    );
    Ok(())
}

pub(super) async fn rejudge(args: EvalRejudgeArgs, cli: CliOverrides) -> Result<()> {
    let write = args.execute || args.cancel;
    let persist = super::connect(cli, write).await?;
    if args.status {
        return status(&persist).await;
    }
    if args.cancel {
        match cancel_active(&persist).await? {
            Some(run) => println!("cancelled re-judge run {}", run.id),
            None => println!("no re-judge run is queued or running"),
        }
        return Ok(());
    }
    let Some(since) = args.since.as_deref() else {
        bail!("mike eval rejudge needs --since (or --status / --cancel)");
    };
    let since = parse("since", since)?;
    let until = args
        .until
        .as_deref()
        .map(|v| parse("until", v))
        .transpose()?
        .unwrap_or_else(Utc::now);
    let overlay = crate::settings::apply_db_rows_to_overlay(persist.fetch_settings().await?);
    if args.role == "session_judge" {
        let plan = plan_session_rejudge(&persist, &overlay, since, until, args.daily_cap).await?;
        println!("{}", serde_json::to_string_pretty(&plan)?);
        if args.execute {
            let id = start_session_rejudge(&persist, &plan, "mike eval rejudge").await?;
            println!("started session-end re-judge run {id}");
        } else {
            println!("Dry run only: pass --execute to start the run. Nothing was written.");
        }
        return Ok(());
    }
    let plan = plan_rejudge(&persist, &overlay, since, until, args.daily_cap).await?;
    print_plan(&plan)?;
    if !args.execute {
        println!("Dry run only: pass --execute to start the run. Nothing was written.");
        return Ok(());
    }
    let id = start_rejudge(&persist, &plan, "mike eval rejudge").await?;
    println!(
        "started re-judge run {id}; the server's judge worker drains it at {} calls a day",
        plan.daily_cap
    );
    Ok(())
}
