use crate::store::persistence::dossier_repo::DossierPersist as _;
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};

use super::dossier_backfill::{BackfillLine, BackfillOptions, BackfillSummary, record};
use crate::dossier::entries::{
    EntriesReport, ledger::plan_builder_rows, link_ledger_step, plan_entries_step, run_entries_step,
};
use crate::dossier::steering_facts;
use crate::dossier::worker::DossierRuntime;
use crate::store::persistence::steering_facts::StaleFilter;
use crate::store::persistence::{PersistError, PersistHandle};

const INTAKE_ASK_LEDGER_LIMIT: i64 = 40;

fn line(
    stage: &'static str,
    ids: &[i64],
    opts: &BackfillOptions,
    started: Instant,
) -> BackfillLine {
    BackfillLine {
        stage,
        dry_run: !opts.execute,
        sessions: ids.len(),
        rows_written: 0,
        reclassified: 0,
        rows_linked: 0,
        rows_unmatched: 0,
        entries_written: 0,
        closes_written: 0,
        elapsed_ms: started.elapsed().as_millis(),
    }
}

async fn builder_rows_planned(
    persist: &PersistHandle,
    ids: &[i64],
) -> Result<EntriesReport, PersistError> {
    let mut report = EntriesReport::default();
    if let Some(since) = persist.dossier().ledger_builder_since().await? {
        let candidates = persist.dossier().load_ledger_candidates(ids, since).await?;
        report.ledger_rows_inserted =
            u64::try_from(plan_builder_rows(&candidates).len()).unwrap_or(0);
    }
    Ok(report)
}

async fn ledger_batch(
    persist: &PersistHandle,
    ids: &[i64],
    opts: &BackfillOptions,
    summary: &mut BackfillSummary,
) -> Result<BackfillLine, PersistError> {
    let started = Instant::now();
    let history = persist
        .dossier()
        .link_ask_ledger_history(ids, opts.execute)
        .await?;
    let builder = if opts.execute {
        let since = persist.dossier().ledger_builder_since().await?;
        link_ledger_step(persist, ids, since).await?
    } else {
        builder_rows_planned(persist, ids).await?
    };
    let recounts = persist
        .dossier()
        .recount_intake_maps(ids, INTAKE_ASK_LEDGER_LIMIT, opts.execute)
        .await?;
    summary.intake_recounts.extend(recounts);
    let mut out = line("ledger", ids, opts, started);
    out.rows_linked = history.rows_linked + builder.ledger_rows_linked;
    out.rows_unmatched = history.rows_unmatched;
    out.rows_written = builder.ledger_rows_inserted;
    out.elapsed_ms = started.elapsed().as_millis();
    Ok(out)
}

async fn entries_batch(
    persist: &PersistHandle,
    ids: &[i64],
    opts: &BackfillOptions,
    settle: Duration,
) -> Result<BackfillLine, PersistError> {
    let started = Instant::now();
    let report = if opts.execute {
        run_entries_step(persist, ids, settle).await?
    } else {
        plan_entries_step(persist, ids, settle).await?
    };
    let mut out = line("entries", ids, opts, started);
    out.entries_written = report.entries_written;
    out.closes_written = report.closes_written;
    out.elapsed_ms = started.elapsed().as_millis();
    Ok(out)
}

pub(super) async fn run_ledger_stage(
    persist: &PersistHandle,
    ids: &[i64],
    opts: &BackfillOptions,
    summary: &mut BackfillSummary,
    emit: &mut impl FnMut(&BackfillLine),
) -> Result<(), PersistError> {
    for chunk in ids.chunks(opts.batch_sessions) {
        let line = ledger_batch(persist, chunk, opts, summary).await?;
        record(summary, &line, chunk.len());
        emit(&line);
    }
    Ok(())
}

pub(super) async fn run_entries_stage(
    persist: &PersistHandle,
    ids: &[i64],
    opts: &BackfillOptions,
    summary: &mut BackfillSummary,
    emit: &mut impl FnMut(&BackfillLine),
) -> Result<(), PersistError> {
    let runtime = DossierRuntime::read(persist, DossierRuntime::default()).await;
    for chunk in ids.chunks(opts.batch_sessions) {
        let line = entries_batch(persist, chunk, opts, runtime.settle).await?;
        record(summary, &line, chunk.len());
        emit(&line);
    }
    summary.entry_counts = persist
        .dossier()
        .count_dossier_entries_since(opts.from)
        .await?;
    Ok(())
}

pub(super) async fn run_steering_facts_stage(
    persist: &PersistHandle,
    opts: &BackfillOptions,
    until: DateTime<Utc>,
    summary: &mut BackfillSummary,
    emit: &mut impl FnMut(&BackfillLine),
) -> Result<(), PersistError> {
    let started = Instant::now();
    let filter = StaleFilter {
        from: Some(opts.from),
        until: Some(until),
        ..StaleFilter::default()
    };
    let report = steering_facts::fill(persist, filter, opts.execute).await?;
    let mut out = line("steering-facts", &[], opts, started);
    out.rows_written = if opts.execute {
        report.written
    } else {
        report.turns
    };
    out.elapsed_ms = started.elapsed().as_millis();
    record(summary, &out, 0);
    emit(&out);
    summary.steering_facts = Some(report);
    Ok(())
}
