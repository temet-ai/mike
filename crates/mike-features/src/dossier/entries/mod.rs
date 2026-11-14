use crate::store::persistence::dossier_repo::DossierPersist as _;
pub mod directives;
pub mod ledger;
pub mod promises;

#[cfg(test)]
pub mod directives_tests;
#[cfg(test)]
pub mod ledger_tests;
#[cfg(test)]
pub mod promises_tests;

use std::sync::Once;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::features_settings::meta_dossier::ENTRIES_ENABLED_KEY;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct EntriesReport {
    pub ledger_rows_inserted: u64,
    pub ledger_rows_linked: u64,
    pub entries_written: u64,
    pub closes_written: u64,
}

impl EntriesReport {
    pub fn absorb(&mut self, other: &EntriesReport) {
        self.ledger_rows_inserted += other.ledger_rows_inserted;
        self.ledger_rows_linked += other.ledger_rows_linked;
        self.entries_written += other.entries_written;
        self.closes_written += other.closes_written;
    }
}

static MISSING_GO_LIVE: Once = Once::new();

fn warn_missing_go_live() {
    MISSING_GO_LIVE.call_once(|| {
        tracing::warn!(
            "dossier entries: the JC03 migration row is missing from _sqlx_migrations; \
             the builder path inserts no ask-ledger rows"
        );
    });
}

pub async fn entries_enabled(persist: &PersistHandle) -> Result<bool, PersistError> {
    let value = persist.setting_value(ENTRIES_ENABLED_KEY).await?;
    Ok(value.and_then(|v| v.as_bool()).unwrap_or(true))
}

pub async fn link_ledger_step(
    persist: &PersistHandle,
    session_ids: &[i64],
    since: Option<DateTime<Utc>>,
) -> Result<EntriesReport, PersistError> {
    let mut report = EntriesReport::default();
    if session_ids.is_empty() {
        return Ok(report);
    }
    match since {
        Some(since) => {
            let candidates = persist
                .dossier()
                .load_ledger_candidates(session_ids, since)
                .await?;
            let rows = ledger::plan_builder_rows(&candidates);
            report.ledger_rows_inserted =
                persist.dossier().insert_builder_ledger_rows(&rows).await?;
        }
        None => warn_missing_go_live(),
    }
    report.ledger_rows_linked = persist
        .dossier()
        .link_ask_ledger_to_turns(session_ids)
        .await?;
    Ok(report)
}

pub async fn run_entries_step(
    persist: &PersistHandle,
    session_ids: &[i64],
    settle: Duration,
) -> Result<EntriesReport, PersistError> {
    run_entries_step_with(persist, session_ids, settle, None).await
}

pub async fn run_entries_step_with(
    persist: &PersistHandle,
    session_ids: &[i64],
    settle: Duration,
    decisions: Option<&crate::llm::decisions::env::DecisionEnv>,
) -> Result<EntriesReport, PersistError> {
    let mut report = EntriesReport::default();
    if session_ids.is_empty() {
        return Ok(report);
    }
    let directive_sources = persist
        .dossier()
        .load_directive_sources(session_ids)
        .await?;
    let directives = crate::dossier::directive_point::refine(
        decisions,
        &directive_sources,
        directives::entries_for(&directive_sources),
    )
    .await;
    report.entries_written += persist
        .dossier()
        .insert_dossier_entries(&directives)
        .await?;
    report.closes_written += persist.dossier().supersede_directives(session_ids).await?;
    let promise_sources = persist.dossier().load_promise_sources(session_ids).await?;
    if !promise_sources.is_empty() {
        let backing = persist.dossier().load_backing_items(session_ids).await?;
        let registry_first = persist.dossier().work_registry_first_launch().await?;
        let promises = promises::open_entries(&promise_sources, &backing, registry_first);
        report.entries_written += persist.dossier().insert_dossier_entries(&promises).await?;
    }
    let closes = persist
        .dossier()
        .promise_close_candidates(session_ids, settle.num_minutes())
        .await?;
    report.closes_written += persist.dossier().close_dossier_entries(&closes).await?;
    Ok(report)
}

pub async fn run_after_build(
    persist: &PersistHandle,
    session_ids: &[i64],
    settle: Duration,
    decisions: Option<&crate::llm::decisions::env::DecisionEnv>,
) -> Result<EntriesReport, PersistError> {
    let since = persist.dossier().ledger_builder_since().await?;
    let mut report = link_ledger_step(persist, session_ids, since).await?;
    if entries_enabled(persist).await? {
        let entries = run_entries_step_with(persist, session_ids, settle, decisions).await?;
        report.absorb(&entries);
    }
    Ok(report)
}

pub async fn plan_entries_step(
    persist: &PersistHandle,
    session_ids: &[i64],
    settle: Duration,
) -> Result<EntriesReport, PersistError> {
    let mut report = EntriesReport::default();
    if session_ids.is_empty() {
        return Ok(report);
    }
    let directive_sources = persist
        .dossier()
        .load_directive_sources(session_ids)
        .await?;
    let directive_count = directives::entries_for(&directive_sources).len();
    let promise_sources = persist.dossier().load_promise_sources(session_ids).await?;
    let closes = persist
        .dossier()
        .promise_close_candidates(session_ids, settle.num_minutes())
        .await?;
    report.entries_written = u64::try_from(directive_count + promise_sources.len()).unwrap_or(0);
    report.closes_written = u64::try_from(closes.len()).unwrap_or(0);
    Ok(report)
}
