use crate::store::persistence::dossier_repo::DossierPersist as _;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::{DossierBackfillArgs, dossier_backfill_entries};
use crate::config::{CliOverrides, load};
use crate::dossier::build::{BuildConfig, build_sessions, classify_stage, plan_sessions};
use crate::dossier::worker::DossierRuntime;
use crate::store::persistence::{PersistError, PersistHandle};

const LANE_BACKFILL_SINCE: &str = "2026-09-17T00:00:00Z";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackfillStage {
    Classify,
    Ledger,
    Facts,
    Entries,
    SteeringFacts,
    All,
}

impl BackfillStage {
    fn includes_classify(self) -> bool {
        matches!(self, BackfillStage::Classify | BackfillStage::All)
    }

    fn includes_ledger(self) -> bool {
        matches!(self, BackfillStage::Ledger | BackfillStage::All)
    }

    fn includes_facts(self) -> bool {
        matches!(self, BackfillStage::Facts | BackfillStage::All)
    }

    fn includes_entries(self) -> bool {
        matches!(self, BackfillStage::Entries | BackfillStage::All)
    }

    fn includes_steering_facts(self) -> bool {
        matches!(self, BackfillStage::SteeringFacts | BackfillStage::All)
    }
}

#[derive(Debug, Clone)]
pub struct BackfillOptions {
    pub from: DateTime<Utc>,
    pub until: Option<DateTime<Utc>>,
    pub stage: BackfillStage,
    pub batch_sessions: usize,
    pub execute: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackfillLine {
    pub stage: &'static str,
    pub dry_run: bool,
    pub sessions: usize,
    pub rows_written: u64,
    pub reclassified: u64,
    pub rows_linked: u64,
    pub rows_unmatched: u64,
    pub entries_written: u64,
    pub closes_written: u64,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BackfillSummary {
    pub dry_run: bool,
    pub batches: u64,
    pub sessions: u64,
    pub classified: u64,
    pub opened_by_filled: u64,
    pub rows_written: u64,
    pub reclassified: u64,
    pub skipped_stale: u64,
    pub rows_linked: u64,
    pub rows_unmatched: u64,
    pub entries_written: u64,
    pub closes_written: u64,
    pub slowest_batch_ms: u128,
    pub intake_recounts: Vec<crate::store::persistence::ask_ledger_backfill::IntakeRecount>,
    pub entry_counts: Vec<crate::store::persistence::dossier_entries::EntryCountRow>,
    pub steering_facts: Option<crate::dossier::steering_facts::FillReport>,
}

fn parse_time(label: &str, value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .with_context(|| format!("--{label} {value:?} is not an RFC 3339 timestamp"))
}

fn parse_stage(value: &str) -> Result<BackfillStage> {
    match value {
        "classify" => Ok(BackfillStage::Classify),
        "ledger" => Ok(BackfillStage::Ledger),
        "facts" => Ok(BackfillStage::Facts),
        "entries" => Ok(BackfillStage::Entries),
        "steering-facts" => Ok(BackfillStage::SteeringFacts),
        "all" => Ok(BackfillStage::All),
        other => bail!(
            "--stage {other:?} is not one of classify, ledger, facts, entries, steering-facts, all"
        ),
    }
}

impl BackfillOptions {
    pub fn from_args(args: &DossierBackfillArgs) -> Result<Self> {
        Ok(Self {
            from: parse_time("from", &args.from)?,
            until: args
                .until
                .as_deref()
                .map(|value| parse_time("until", value))
                .transpose()?,
            stage: parse_stage(&args.stage)?,
            batch_sessions: usize::try_from(args.batch_sessions.max(1)).unwrap_or(1),
            execute: args.execute,
        })
    }
}

pub(super) fn record(summary: &mut BackfillSummary, line: &BackfillLine, sessions: usize) {
    summary.batches += 1;
    summary.sessions += sessions as u64;
    summary.rows_written += line.rows_written;
    summary.reclassified += line.reclassified;
    summary.rows_linked += line.rows_linked;
    summary.rows_unmatched += line.rows_unmatched;
    summary.entries_written += line.entries_written;
    summary.closes_written += line.closes_written;
    summary.slowest_batch_ms = summary.slowest_batch_ms.max(line.elapsed_ms);
}

async fn classify_batch(
    persist: &PersistHandle,
    ids: &[i64],
    opts: &BackfillOptions,
    now: DateTime<Utc>,
    summary: &mut BackfillSummary,
) -> Result<BackfillLine, PersistError> {
    let started = Instant::now();
    let outcome = classify_stage(persist, ids, now, opts.execute).await?;
    summary.classified += outcome.classified;
    summary.opened_by_filled += outcome.opened_by;
    Ok(BackfillLine {
        stage: "classify",
        dry_run: !opts.execute,
        sessions: ids.len(),
        rows_written: outcome.classified + outcome.opened_by,
        reclassified: 0,
        rows_linked: 0,
        rows_unmatched: 0,
        entries_written: 0,
        closes_written: 0,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

async fn facts_batch(
    persist: &PersistHandle,
    ids: &[i64],
    opts: &BackfillOptions,
    now: DateTime<Utc>,
    summary: &mut BackfillSummary,
) -> Result<BackfillLine, PersistError> {
    let started = Instant::now();
    let runtime = DossierRuntime::read(persist, DossierRuntime::default()).await;
    let cfg = BuildConfig {
        settle: runtime.settle,
    };
    let (rows_written, reclassified, skipped) = if opts.execute {
        let report = build_sessions(persist, ids, now, &cfg).await?;
        (
            report.rows_written(),
            report.reclassified,
            report.skipped_stale,
        )
    } else {
        let plan = plan_sessions(persist, ids, now, &cfg).await?;
        (plan.would_insert + plan.would_update, 0, plan.skipped_stale)
    };
    summary.skipped_stale += skipped;
    Ok(BackfillLine {
        stage: "facts",
        dry_run: !opts.execute,
        sessions: ids.len(),
        rows_written,
        reclassified,
        rows_linked: 0,
        rows_unmatched: 0,
        entries_written: 0,
        closes_written: 0,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

async fn run_classify_stage(
    persist: &PersistHandle,
    opts: &BackfillOptions,
    (now, until): (DateTime<Utc>, DateTime<Utc>),
    summary: &mut BackfillSummary,
    emit: &mut impl FnMut(&BackfillLine),
) -> Result<(), PersistError> {
    let ids = persist
        .dossier()
        .backfill_sessions_with_prompts(opts.from, until)
        .await?;
    for chunk in ids.chunks(opts.batch_sessions) {
        let line = classify_batch(persist, chunk, opts, now, summary).await?;
        record(summary, &line, chunk.len());
        emit(&line);
    }
    Ok(())
}

async fn turn_sessions_for(
    persist: &PersistHandle,
    opts: &BackfillOptions,
    until: DateTime<Utc>,
) -> Result<Vec<i64>, PersistError> {
    let stage = &opts.stage;
    if stage.includes_ledger() || stage.includes_facts() || stage.includes_entries() {
        persist
            .dossier()
            .backfill_sessions_with_turns(opts.from, until)
            .await
    } else {
        Ok(Vec::new())
    }
}

async fn run_facts_stage(
    persist: &PersistHandle,
    turn_sessions: &[i64],
    (opts, now): (&BackfillOptions, DateTime<Utc>),
    summary: &mut BackfillSummary,
    emit: &mut impl FnMut(&BackfillLine),
) -> Result<(), PersistError> {
    for chunk in turn_sessions.chunks(opts.batch_sessions) {
        let line = facts_batch(persist, chunk, opts, now, summary).await?;
        record(summary, &line, chunk.len());
        emit(&line);
    }
    Ok(())
}

pub async fn run_backfill(
    persist: &PersistHandle,
    opts: &BackfillOptions,
    mut emit: impl FnMut(&BackfillLine),
) -> Result<BackfillSummary, PersistError> {
    let now = Utc::now();
    let until = opts.until.unwrap_or(now);
    let mut summary = BackfillSummary {
        dry_run: !opts.execute,
        ..BackfillSummary::default()
    };
    if opts.stage.includes_classify() {
        run_classify_stage(persist, opts, (now, until), &mut summary, &mut emit).await?;
    }
    let turn_sessions = turn_sessions_for(persist, opts, until).await?;
    if opts.stage.includes_ledger() {
        dossier_backfill_entries::run_ledger_stage(
            persist,
            &turn_sessions,
            opts,
            &mut summary,
            &mut emit,
        )
        .await?;
    }
    if opts.stage.includes_facts() {
        run_facts_stage(
            persist,
            &turn_sessions,
            (opts, now),
            &mut summary,
            &mut emit,
        )
        .await?;
    }
    if opts.stage.includes_entries() {
        dossier_backfill_entries::run_entries_stage(
            persist,
            &turn_sessions,
            opts,
            &mut summary,
            &mut emit,
        )
        .await?;
    }
    if opts.stage.includes_steering_facts() {
        dossier_backfill_entries::run_steering_facts_stage(
            persist,
            opts,
            until,
            &mut summary,
            &mut emit,
        )
        .await?;
    }
    Ok(summary)
}

async fn refuse_until_lanes_are_clean(persist: &PersistHandle) -> Result<()> {
    let since = parse_time("since", LANE_BACKFILL_SINCE)?;
    let remaining = persist
        .dossier()
        .count_primary_requests_with_agent_id(since)
        .await
        .context("dossier-backfill: could not count agent-id requests on the primary lane")?;
    if remaining > 0 {
        bail!(
            "dossier-backfill --execute refused: {remaining} request(s) since {LANE_BACKFILL_SINCE} \
             still carry an agent id on the primary lane; run 'mike origin-backfill --execute' first"
        );
    }
    Ok(())
}

pub async fn guard_execute(persist: &PersistHandle, opts: &BackfillOptions) -> Result<()> {
    if opts.execute {
        refuse_until_lanes_are_clean(persist).await?;
    }
    Ok(())
}

pub(super) async fn dossier_backfill(args: DossierBackfillArgs, cli: CliOverrides) -> Result<()> {
    let opts = BackfillOptions::from_args(&args)?;
    let config = load::load(cli).context("failed to resolve config for dossier-backfill")?;
    let Some(database_url) = config.database_url.as_deref() else {
        bail!(
            "dossier-backfill requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };
    let handle = if opts.execute {
        PersistHandle::connect_existing(database_url).await
    } else {
        PersistHandle::connect_read_only(database_url).await
    }
    .context("dossier-backfill: could not connect to the target database")?;
    guard_execute(&handle, &opts).await?;
    let summary = run_backfill(&handle, &opts, |line| {
        if let Ok(text) = serde_json::to_string(line) {
            println!("{text}");
        }
    })
    .await
    .context("dossier-backfill: batch failed")?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    if !opts.execute {
        println!("Dry run only — pass --execute to write. No rows were written.");
    }
    Ok(())
}
