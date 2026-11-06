pub mod assemble;
pub mod checks;
pub mod classify_step;
pub mod ended_on;
mod load_step;
pub mod resolve;
pub mod session_class;
mod session_step;

pub(crate) use session_step::classify_session_as_of;

#[cfg(test)]
mod checks_tests;
#[cfg(test)]
mod ended_on_tests;
#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
mod session_class_tests;

use chrono::{DateTime, Duration, Utc};

use crate::dossier::genuine::CLASSIFIER_VERSION;
use crate::store::persistence::turn_facts::{OpenedByRow, TurnFactsRow};
use crate::store::persistence::{PersistError, PersistHandle};

pub const BUILDER_VERSION: &str = "turn-facts-v1";

const UPSERT_CHUNK: usize = 500;

#[derive(Debug, Clone, Copy)]
pub struct BuildConfig {
    pub settle: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildReport {
    pub classified: u64,
    pub opened_by_filled: u64,
    pub inserted: u64,
    pub updated: u64,
    pub reclassified: u64,
    pub skipped_stale: u64,
    pub stamped: u64,
    pub entries: crate::dossier::entries::EntriesReport,
    pub steering_facts: u64,
}

impl BuildReport {
    pub fn rows_written(&self) -> u64 {
        self.inserted + self.updated
    }

    pub fn absorb(&mut self, other: &BuildReport) {
        self.classified += other.classified;
        self.opened_by_filled += other.opened_by_filled;
        self.inserted += other.inserted;
        self.updated += other.updated;
        self.reclassified += other.reclassified;
        self.skipped_stale += other.skipped_stale;
        self.stamped += other.stamped;
        self.entries.absorb(&other.entries);
        self.steering_facts += other.steering_facts;
    }
}

#[derive(Debug, Clone, Default)]
pub struct BuildPlan {
    pub rows: Vec<TurnFactsRow>,
    pub unchanged_turn_ids: Vec<i64>,
    pub opened_by: Vec<OpenedByRow>,
    pub pending_classifications: u64,
    pub would_insert: u64,
    pub would_update: u64,
    pub skipped_stale: u64,
}

pub async fn plan_sessions(
    persist: &PersistHandle,
    session_ids: &[i64],
    now: DateTime<Utc>,
    cfg: &BuildConfig,
) -> Result<BuildPlan, PersistError> {
    let mut plan = BuildPlan::default();
    if session_ids.is_empty() {
        return Ok(plan);
    }
    let pending = persist
        .load_prompts_needing_classification(session_ids, CLASSIFIER_VERSION)
        .await?;
    plan.pending_classifications = pending.len() as u64;
    let mut loaded = load_step::load_batch(persist, session_ids, now).await?;
    plan.opened_by = classify_step::derive_opened_by(&loaded.streams, &mut loaded.turns);
    let mut fresh = load_step::load_fresh(persist, &loaded, session_ids).await?;
    let stale = load_step::settled_stale_final_texts(&loaded, &fresh, now - cfg.settle);
    if persist.refresh_final_assistant_text(&stale).await? > 0 {
        fresh = load_step::load_fresh(persist, &loaded, session_ids).await?;
    }
    for session_id in session_ids {
        let built = session_step::build_session_rows(&loaded, &fresh, *session_id, now, cfg);
        plan.skipped_stale += built.skipped_stale;
        for row in built.rows {
            match loaded.existing.get(&row.operator_turn_id) {
                None => plan.would_insert += 1,
                Some(old) if assemble::materially_differs(old, &row) => plan.would_update += 1,
                Some(_) => {
                    plan.unchanged_turn_ids.push(row.operator_turn_id);
                    continue;
                }
            }
            plan.rows.push(row);
        }
    }
    Ok(plan)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassifyOutcome {
    pub classified: u64,
    pub opened_by: u64,
}

pub async fn classify_stage(
    persist: &PersistHandle,
    session_ids: &[i64],
    now: DateTime<Utc>,
    execute: bool,
) -> Result<ClassifyOutcome, PersistError> {
    if !execute {
        let pending = persist
            .load_prompts_needing_classification(session_ids, CLASSIFIER_VERSION)
            .await?;
        let turns = persist.load_turn_records(session_ids).await?;
        return Ok(ClassifyOutcome {
            classified: pending.len() as u64,
            opened_by: turns.iter().filter(|turn| turn.opened_by.is_none()).count() as u64,
        });
    }
    let classified = classify_step::ensure_classifications(persist, session_ids).await?;
    let (streams, mut turns) = load_step::load_streams_and_turns(persist, session_ids, now).await?;
    let rows = classify_step::derive_opened_by(&streams, &mut turns);
    let opened_by = persist.fill_operator_turn_opened_by(&rows).await?;
    Ok(ClassifyOutcome {
        classified,
        opened_by,
    })
}

pub async fn build_sessions(
    persist: &PersistHandle,
    session_ids: &[i64],
    now: DateTime<Utc>,
    cfg: &BuildConfig,
) -> Result<BuildReport, PersistError> {
    let mut report = BuildReport::default();
    if session_ids.is_empty() {
        return Ok(report);
    }
    report.classified = classify_step::ensure_classifications(persist, session_ids).await?;
    let plan = plan_sessions(persist, session_ids, now, cfg).await?;
    report.opened_by_filled = persist
        .fill_operator_turn_opened_by(&plan.opened_by)
        .await?;
    report.skipped_stale = plan.skipped_stale;
    for chunk in plan.rows.chunks(UPSERT_CHUNK) {
        let outcome = persist.upsert_turn_facts(chunk).await?;
        report.inserted += outcome.inserted;
        report.updated += outcome.updated;
        report.reclassified += outcome.reclassified;
    }
    report.stamped = persist
        .stamp_turn_facts_built_at(&plan.unchanged_turn_ids, now)
        .await?;
    report.entries =
        crate::dossier::entries::run_after_build(persist, session_ids, cfg.settle).await?;
    report.steering_facts = crate::dossier::steering_facts::for_closed_turns(persist, session_ids)
        .await?
        .written;
    Ok(report)
}
