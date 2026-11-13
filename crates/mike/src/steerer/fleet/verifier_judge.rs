use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use tracing::{debug, warn};
use uuid::Uuid;

use super::input_budget::{RawVerifierContext, VerifierInputBudget, project_verifier_context};
use crate::steerer::verifier::{
    build_accumulated_ask, build_trajectory_digest, written_by_the_operator,
};
use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;
use crate::store::persistence::session_action_list::{SessionActionCounts, SessionActionRow};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Default)]
pub(super) struct VerifierJudgeContext {
    pub(super) accumulated_ask: String,
    pub(super) trajectory_digest: String,
    pub(super) prior_fires_digest: String,
    pub(super) thread_criteria: Option<String>,
    pub(super) intake_criteria: Option<String>,
    pub(super) session_action_list: String,
    pub(super) budget: VerifierInputBudget,
}

const LEDGER_FETCH_LIMIT: i64 = 30;

async fn fetch_ask_entries(ph: &PersistHandle, session_uuid: Uuid) -> Vec<AskLedgerEntryRow> {
    match ph
        .dossier()
        .fetch_ask_ledger_for_model(session_uuid, LEDGER_FETCH_LIMIT)
        .await
    {
        Ok(entries) => written_by_the_operator(entries),
        Err(e) => {
            warn!(
                session_id = %session_uuid,
                error = %e,
                "steerer fleet: ask-ledger fetch failed — judge sees no accumulated ask"
            );
            Vec::new()
        }
    }
}

fn log_latest_turn_lookup_failed(session_uuid: Uuid, e: &PersistError) {
    warn!(
        session_id = %session_uuid,
        error = %e,
        "steerer fleet: latest-turn lookup failed — judge sees no trajectory digest"
    );
}

fn log_turn_tool_calls_fetch_failed(session_uuid: Uuid, turn_id: i64, e: &PersistError) {
    warn!(
        session_id = %session_uuid,
        turn_id,
        error = %e,
        "steerer fleet: turn tool-calls fetch failed — judge sees no trajectory digest"
    );
}

async fn resolve_digest_turn_id(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: i64,
    turn_id_override: Option<i64>,
) -> Option<i64> {
    if let Some(turn_id) = turn_id_override {
        return Some(turn_id);
    }
    match ph.latest_turn_id_for_session(session_db_id).await {
        Ok(turn_id) => turn_id,
        Err(e) => {
            log_latest_turn_lookup_failed(session_uuid, &e);
            None
        }
    }
}

async fn fetch_trajectory_digest(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: Option<i64>,
    turn_id_override: Option<i64>,
) -> String {
    let Some(session_db_id) = session_db_id else {
        return String::new();
    };
    let Some(turn_id) =
        resolve_digest_turn_id(ph, session_uuid, session_db_id, turn_id_override).await
    else {
        return String::new();
    };
    match ph
        .steerer()
        .fetch_turn_tool_calls_paired(turn_id, session_db_id)
        .await
    {
        Ok(rows) => build_trajectory_digest(&rows),
        Err(e) => {
            log_turn_tool_calls_fetch_failed(session_uuid, turn_id, &e);
            String::new()
        }
    }
}

fn log_action_list_fetch_failed(session_uuid: Uuid, e: &PersistError) {
    warn!(
        session_id = %session_uuid,
        error = %e,
        "steerer fleet: session action-list fetch failed — judge sees no session action list"
    );
}

async fn fetch_action_rows(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: Option<i64>,
    budget: VerifierInputBudget,
) -> (Vec<SessionActionRow>, Option<SessionActionCounts>) {
    let Some(session_db_id) = session_db_id else {
        return (Vec::new(), None);
    };
    let Some(limit) = budget.action_rows_loaded() else {
        return match ph.steerer().fetch_session_action_list(session_db_id).await {
            Ok(rows) => (rows, None),
            Err(e) => {
                log_action_list_fetch_failed(session_uuid, &e);
                (Vec::new(), None)
            }
        };
    };
    let (rows, counts) = tokio::join!(
        ph.steerer()
            .fetch_session_action_list_recent(session_db_id, limit),
        ph.steerer().fetch_session_action_counts(session_db_id),
    );
    match choose_action_source(rows, counts) {
        ActionSource::Recent(rows, counts) => (rows, Some(counts)),
        ActionSource::FullList(e) => {
            log_action_counts_unavailable(session_uuid, &e);
            fetch_full_action_rows(ph, session_uuid, session_db_id).await
        }
        ActionSource::None(e) => {
            log_action_list_fetch_failed(session_uuid, &e);
            (Vec::new(), None)
        }
    }
}

async fn fetch_full_action_rows(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: i64,
) -> (Vec<SessionActionRow>, Option<SessionActionCounts>) {
    match ph.steerer().fetch_session_action_list(session_db_id).await {
        Ok(rows) => (rows, None),
        Err(e) => {
            log_action_list_fetch_failed(session_uuid, &e);
            (Vec::new(), None)
        }
    }
}

enum ActionSource {
    Recent(Vec<SessionActionRow>, SessionActionCounts),
    FullList(PersistError),
    None(PersistError),
}

fn choose_action_source(
    rows: Result<Vec<SessionActionRow>, PersistError>,
    counts: Result<SessionActionCounts, PersistError>,
) -> ActionSource {
    match (rows, counts) {
        (Ok(rows), Ok(counts)) => ActionSource::Recent(rows, counts),
        (Ok(_), Err(e)) => ActionSource::FullList(e),
        (Err(e), _) => ActionSource::None(e),
    }
}

fn log_action_counts_unavailable(session_uuid: Uuid, e: &PersistError) {
    warn!(
        session_id = %session_uuid,
        error = %e,
        "steerer fleet: session action counts unavailable — loading the full action list for this evaluation so no omitted calls go unmarked"
    );
}

async fn fetch_thread_criteria(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: Option<i64>,
    accumulated_ask: &str,
) -> Option<String> {
    let session_db_id = session_db_id?;
    match ph
        .launch()
        .fetch_thread_criteria_for_session(session_db_id, accumulated_ask)
        .await
    {
        Ok(found) => found.map(|thread| {
            debug!(
                session_id = %session_uuid,
                thread_slug = %thread.thread_slug,
                found_via = ?thread.via,
                "steerer fleet: acceptance criteria read from the work thread"
            );
            thread.criteria_text
        }),
        Err(e) => {
            warn!(
                session_id = %session_uuid,
                error = %e,
                "steerer fleet: work-thread criteria lookup failed — falling back to the generated checklist"
            );
            None
        }
    }
}

async fn fetch_intake_criteria(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: Option<i64>,
) -> Option<String> {
    let session_db_id = session_db_id?;
    match ph
        .steerer()
        .fetch_current_acceptance_criteria(session_db_id)
        .await
    {
        Ok(Some(map)) if !map.criteria.is_empty() => Some(render_intake_criteria(&map)),
        Ok(_) => None,
        Err(e) => {
            warn!(
                session_id = %session_uuid,
                error = %e,
                "steerer fleet: intake-criteria lookup failed — judge generates its own checklist"
            );
            None
        }
    }
}

async fn load_raw_verifier_context(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: Option<i64>,
    turn_id_override: Option<i64>,
    budget: VerifierInputBudget,
) -> RawVerifierContext {
    let ask_entries = fetch_ask_entries(ph, session_uuid).await;
    let trajectory_digest =
        fetch_trajectory_digest(ph, session_uuid, session_db_id, turn_id_override).await;
    let (action_rows, action_counts) =
        fetch_action_rows(ph, session_uuid, session_db_id, budget).await;
    let full_ask = build_accumulated_ask(&ask_entries);
    let thread_criteria = fetch_thread_criteria(ph, session_uuid, session_db_id, &full_ask).await;
    let intake_criteria = if thread_criteria.is_some() {
        None
    } else {
        fetch_intake_criteria(ph, session_uuid, session_db_id).await
    };
    RawVerifierContext {
        ask_entries,
        trajectory_digest,
        action_rows,
        action_counts,
        thread_criteria,
        intake_criteria,
    }
}

pub(super) async fn build_verifier_judge_context(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: Option<i64>,
    turn_id_override: Option<i64>,
    budget: VerifierInputBudget,
) -> VerifierJudgeContext {
    let raw =
        load_raw_verifier_context(ph, session_uuid, session_db_id, turn_id_override, budget).await;
    project_verifier_context(&raw, budget)
}

pub(super) fn render_intake_criteria(
    map: &crate::store::persistence::acceptance_criteria::AcceptanceCriteriaMap,
) -> String {
    let mut out = String::from(
        "Acceptance criteria for this session, written from the operator's own words by a \
         worker that never saw this session's plan or restatement.\n",
    );
    for c in &map.criteria {
        match (c.provenance.as_str(), c.operator_span.as_deref()) {
            ("operator", Some(span)) => out.push_str(&format!(
                "\n- [operator asked] {}\n  (their words: \"{}\")",
                c.criterion.trim(),
                span.trim()
            )),
            _ => out.push_str(&format!(
                "\n- [inferred, not stated by the operator] {}",
                c.criterion.trim()
            )),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_intake_criteria_keeps_provenance_per_criterion() {
        use crate::store::persistence::acceptance_criteria::{
            AcceptanceCriteriaMap, AcceptanceCriterionRow,
        };
        let map = AcceptanceCriteriaMap {
            id: 1,
            version: 1,
            ask_entry_count: 2,
            model: Some("m".to_string()),
            output_tokens: None,
            created_at: chrono::Utc::now(),
            criteria: vec![
                AcceptanceCriterionRow {
                    ord: 0,
                    criterion: "The panel is reachable from the session page.".to_string(),
                    provenance: "operator".to_string(),
                    operator_span: Some("it should not need a direct URL".to_string()),
                },
                AcceptanceCriterionRow {
                    ord: 1,
                    criterion: "It uses the current shared primitives.".to_string(),
                    provenance: "inferred".to_string(),
                    operator_span: None,
                },
            ],
        };
        let rendered = render_intake_criteria(&map);
        assert!(rendered.contains("[operator asked] The panel is reachable"));
        assert!(rendered.contains("their words: \"it should not need a direct URL\""));
        assert!(rendered.contains("[inferred, not stated by the operator] It uses the current"));
        assert!(
            rendered.contains("never saw this session's plan or restatement"),
            "the judge should know where these came from"
        );
    }

    fn no_counts() -> SessionActionCounts {
        SessionActionCounts {
            total: 0,
            reads: 0,
            writes: 0,
            commands: 0,
            failed_commands: 0,
        }
    }

    #[test]
    fn counts_failure_with_recent_rows_falls_back_to_the_full_list() {
        let counts_err = || PersistError::DataInvariant("counts unavailable".to_owned());
        assert!(matches!(
            choose_action_source(Ok(Vec::new()), Err(counts_err())),
            ActionSource::FullList(_)
        ));
        assert!(matches!(
            choose_action_source(Err(counts_err()), Ok(no_counts())),
            ActionSource::None(_)
        ));
        assert!(matches!(
            choose_action_source(Ok(Vec::new()), Ok(no_counts())),
            ActionSource::Recent(_, _)
        ));
    }
}
