use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::OriginBackfillArgs;
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::origin_backfill::RelanePlanRow;

#[derive(Debug, Default, Serialize)]
struct DaySummary {
    relaned_workflow_subagent: u64,
    relaned_subagent: u64,
    author_before: BTreeMap<String, u64>,
    author_after: BTreeMap<String, u64>,
    turns_marked_agent: u64,
}

#[derive(Debug, Default, Serialize)]
struct BackfillSummary {
    executed: bool,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    candidate_requests: u64,
    requests_relaned: u64,
    turns_marked_agent: u64,
    closed_by_agent_turns: u64,
    per_day: BTreeMap<String, DaySummary>,
}

fn append_jsonl<T: Serialize>(file: &mut std::fs::File, records: &[T]) -> Result<()> {
    for record in records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    Ok(())
}

fn create(path: &Path) -> Result<std::fs::File> {
    std::fs::File::create(path).with_context(|| format!("could not create {}", path.display()))
}

fn tally_plan(summary: &mut BackfillSummary, plan: &[RelanePlanRow]) {
    for row in plan {
        let day = summary
            .per_day
            .entry(row.created_at.date_naive().to_string())
            .or_default();
        if row.new_origin == "workflow_subagent" {
            day.relaned_workflow_subagent += 1;
        } else {
            day.relaned_subagent += 1;
        }
        *day.author_before
            .entry(row.old_author.clone().unwrap_or_else(|| "null".to_string()))
            .or_default() += 1;
        *day.author_after.entry(row.new_author.clone()).or_default() += 1;
    }
}

pub(super) async fn origin_backfill(args: OriginBackfillArgs, cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for origin-backfill")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "origin-backfill requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };
    let ph = if args.execute {
        PersistHandle::connect_existing(database_url).await
    } else {
        PersistHandle::connect_read_only(database_url).await
    }
    .context("origin-backfill: could not connect to the capture database")?;
    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("could not create {}", args.out.display()))?;
    let until = args.until.unwrap_or_else(Utc::now);
    let window = (args.since, until);
    let mut summary = BackfillSummary {
        executed: args.execute,
        since: Some(args.since),
        until: Some(until),
        ..BackfillSummary::default()
    };
    let mut relaned_file = create(&args.out.join("relaned.jsonl"))?;
    let mut opened_file = create(&args.out.join("agent-opened.jsonl"))?;
    let mut after = (args.since, 0_i64);
    loop {
        let batch = ph
            .origin_backfill_candidates(window, after, args.batch)
            .await?;
        let Some(last) = batch.last() else { break };
        after = (last.1, last.0);
        summary.candidate_requests += batch.len() as u64;
        let ids: Vec<i64> = batch.iter().map(|(id, _)| *id).collect();
        let plan = ph.origin_backfill_plan(&ids).await?;
        tally_plan(&mut summary, &plan);
        let unmarked = ph.origin_backfill_unmarked_turns(&ids).await?;
        for turn in &unmarked {
            summary
                .per_day
                .entry(turn.started_at.date_naive().to_string())
                .or_default()
                .turns_marked_agent += 1;
        }
        summary.turns_marked_agent += unmarked.len() as u64;
        if args.execute {
            summary.requests_relaned += ph.origin_backfill_relane(&plan, window).await?;
            let turn_ids: Vec<i64> = unmarked.iter().map(|turn| turn.turn_id).collect();
            ph.origin_backfill_mark_agent_opened(&turn_ids).await?;
            append_jsonl(&mut relaned_file, &plan)?;
            append_jsonl(&mut opened_file, &unmarked)?;
        }
    }
    let closed = ph.origin_backfill_closed_by_agent(window).await?;
    summary.closed_by_agent_turns = closed.len() as u64;
    append_jsonl(
        &mut create(&args.out.join("closed-by-agent.jsonl"))?,
        &closed,
    )?;
    std::fs::write(
        args.out.join("summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )
    .context("could not write summary.json")?;
    println!(
        "origin-backfill ({}): {} candidate requests, {} re-laned, {} turns marked agent, {} closed by an agent",
        if args.execute { "execute" } else { "dry run" },
        summary.candidate_requests,
        summary.requests_relaned,
        summary.turns_marked_agent,
        summary.closed_by_agent_turns
    );
    Ok(())
}
