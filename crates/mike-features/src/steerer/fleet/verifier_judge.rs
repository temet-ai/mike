use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use tracing::{debug, warn};
use uuid::Uuid;

use super::input_budget::{RawVerifierContext, VerifierInputBudget, project_verifier_context};
use crate::steerer::checklist_point::{Criterion, session_criteria};
use crate::steerer::verifier::{
    build_accumulated_ask, build_trajectory_digest, written_by_the_operator,
};
use crate::store::persistence::acceptance_criteria::AcceptanceCriteriaMap;
use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;
use crate::store::persistence::session_action_list::{SessionActionCounts, SessionActionRow};
use crate::store::persistence::{PersistError, PersistHandle};
use mike_launch::store::persistence::work_threads::criteria::{CriteriaSource, ThreadCriteria};

#[derive(Default)]
pub struct VerifierJudgeContext {
    pub accumulated_ask: String,
    pub trajectory_digest: String,
    pub prior_fires_digest: String,
    pub thread_criteria: Option<String>,
    pub intake_criteria: Option<String>,
    pub checklist_criteria: Vec<Criterion>,
    pub session_action_list: String,
    pub budget: VerifierInputBudget,
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
) -> Option<ThreadCriteria> {
    let session_db_id = session_db_id?;
    match ph
        .launch()
        .fetch_thread_criteria_for_session(session_db_id, accumulated_ask)
        .await
    {
        Ok(found) => found.inspect(|thread| {
            debug!(
                session_id = %session_uuid,
                thread_slug = %thread.thread_slug,
                found_via = ?thread.via,
                "steerer fleet: acceptance criteria read from the work thread"
            );
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

async fn fetch_intake_map(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: Option<i64>,
) -> Option<AcceptanceCriteriaMap> {
    let session_db_id = session_db_id?;
    match ph
        .steerer()
        .fetch_current_acceptance_criteria(session_db_id)
        .await
    {
        Ok(map) => map,
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
    let thread = fetch_thread_criteria(ph, session_uuid, session_db_id, &full_ask).await;
    let intake = if is_linked(thread.as_ref()) {
        None
    } else {
        fetch_intake_map(ph, session_uuid, session_db_id).await
    };
    let criteria = CriteriaFields::from_sources(thread, intake);
    RawVerifierContext {
        ask_entries,
        trajectory_digest,
        action_rows,
        action_counts,
        thread_criteria: criteria.thread_criteria,
        intake_criteria: criteria.intake_criteria,
        checklist_criteria: criteria.checklist_criteria,
    }
}

pub fn is_linked(thread: Option<&ThreadCriteria>) -> bool {
    thread.is_some_and(|t| t.via == CriteriaSource::LinkedSession)
}

pub struct CriteriaFields {
    pub thread_criteria: Option<String>,
    pub intake_criteria: Option<String>,
    pub checklist_criteria: Vec<Criterion>,
}

impl CriteriaFields {
    pub fn from_sources(
        thread: Option<ThreadCriteria>,
        intake: Option<AcceptanceCriteriaMap>,
    ) -> Self {
        let intake = intake.filter(|m| !m.criteria.is_empty());
        let linked = thread
            .as_ref()
            .filter(|t| is_linked(Some(t)))
            .map(|t| t.criteria_text.as_str());
        let checklist_criteria = session_criteria(linked, intake.as_ref());
        let thread_criteria = thread.map(|t| t.criteria_text);
        let intake_criteria = intake
            .filter(|_| thread_criteria.is_none())
            .map(|m| render_intake_criteria(&m));
        Self {
            thread_criteria,
            intake_criteria,
            checklist_criteria,
        }
    }
}

pub async fn build_verifier_judge_context(
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

pub fn render_intake_criteria(map: &AcceptanceCriteriaMap) -> String {
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

    fn one_row_map() -> AcceptanceCriteriaMap {
        use crate::store::persistence::acceptance_criteria::AcceptanceCriterionRow;
        AcceptanceCriteriaMap {
            id: 1,
            version: 1,
            ask_entry_count: 1,
            model: None,
            output_tokens: None,
            created_at: chrono::Utc::now(),
            criteria: vec![AcceptanceCriterionRow {
                ord: 0,
                criterion: "This session's own criterion.".to_string(),
                provenance: "operator".to_string(),
                operator_span: Some("my words".to_string()),
            }],
        }
    }

    fn thread(via: CriteriaSource) -> ThreadCriteria {
        ThreadCriteria {
            thread_slug: "other-work".to_string(),
            criteria_text: "- Another thread's criterion.".to_string(),
            via,
        }
    }

    #[test]
    fn criteria_fields_mentioned_thread_never_reaches_the_checklist() {
        let fields = CriteriaFields::from_sources(
            Some(thread(CriteriaSource::MentionedInAsk)),
            Some(one_row_map()),
        );
        let texts: Vec<&str> = fields
            .checklist_criteria
            .iter()
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(texts, vec!["This session's own criterion."]);
        assert_eq!(
            fields.thread_criteria.as_deref(),
            Some("- Another thread's criterion."),
            "the judge prompt keeps what it saw before"
        );
        assert!(fields.intake_criteria.is_none());
    }

    #[test]
    fn criteria_fields_linked_thread_feeds_the_checklist() {
        let fields =
            CriteriaFields::from_sources(Some(thread(CriteriaSource::LinkedSession)), None);
        let texts: Vec<&str> = fields
            .checklist_criteria
            .iter()
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(texts, vec!["Another thread's criterion."]);
    }

    #[test]
    fn criteria_fields_intake_only_renders_for_the_judge_and_keeps_rows_for_the_checklist() {
        let fields = CriteriaFields::from_sources(None, Some(one_row_map()));
        assert_eq!(fields.checklist_criteria.len(), 1);
        assert!(!fields.checklist_criteria[0].inferred);
        assert!(
            fields
                .intake_criteria
                .as_deref()
                .is_some_and(|t| t.contains("[operator asked] This session's own criterion."))
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
