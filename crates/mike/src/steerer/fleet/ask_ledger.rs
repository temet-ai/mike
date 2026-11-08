use tracing::{debug, info, warn};
use uuid::Uuid;

use super::context::FleetCtx;
use crate::steerer::intake_criteria;
use crate::steerer::verifier::{classify_ask_kind, is_operator_ask, written_by_the_operator};
use crate::store::persistence::acceptance_criteria::{AskSeen, NewCriterion};
use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;
use crate::store::persistence::{PersistError, PersistHandle};

fn log_session_not_resolved_for_ledger_append(session_uuid: Uuid) {
    debug!(
        session_id = %session_uuid,
        "steerer fleet: ask-ledger append — session not resolved yet, skipping"
    );
}

fn log_session_resolve_failed_for_ledger_append(session_uuid: Uuid, e: &PersistError) {
    warn!(
        session_id = %session_uuid,
        error = %e,
        "steerer fleet: ask-ledger append — session resolve failed"
    );
}

async fn resolve_session_db_id_for_ledger(ph: &PersistHandle, session_uuid: Uuid) -> Option<i64> {
    match ph.resolve_session_db_id(&session_uuid.to_string()).await {
        Ok(Some(id)) => Some(id),
        Ok(None) => {
            log_session_not_resolved_for_ledger_append(session_uuid);
            None
        }
        Err(e) => {
            log_session_resolve_failed_for_ledger_append(session_uuid, &e);
            None
        }
    }
}

async fn is_first_ask_ledger_entry(ph: &PersistHandle, session_uuid: Uuid) -> bool {
    match ph.fetch_ask_ledger(session_uuid, 1).await {
        Ok(rows) => rows.is_empty(),
        Err(e) => {
            warn!(
                session_id = %session_uuid,
                error = %e,
                "steerer fleet: ask-ledger append — existing-entries check failed, assuming not-first"
            );
            false
        }
    }
}

pub(super) async fn append_ask_ledger_entry_for_turn(
    ph: &PersistHandle,
    session_uuid: Uuid,
    raw_id: i64,
    text: &str,
) {
    if !is_operator_ask(text) {
        return;
    }
    let Some(session_db_id) = resolve_session_db_id_for_ledger(ph, session_uuid).await else {
        return;
    };
    let is_first = is_first_ask_ledger_entry(ph, session_uuid).await;
    let kind = classify_ask_kind(is_first, text);
    let entry = crate::store::persistence::memory_ask_ledger::NewAskLedgerEntry {
        session_id: session_db_id,
        request_id: Some(raw_id),
        operator_turn_id: None,
        kind,
        kind_source: "keyword",
        content: text,
    };
    if let Err(e) = ph.append_ask_ledger_entry(&entry).await {
        warn!(
            session_id = %session_uuid,
            kind,
            error = %e,
            "steerer fleet: ask-ledger append failed"
        );
    }
}

const INTAKE_ASK_LEDGER_LIMIT: i64 = 40;

struct LedgerForAuthoring {
    entries: Vec<AskLedgerEntryRow>,
    fetched: usize,
}

impl LedgerForAuthoring {
    fn newest_entry_id(&self) -> Option<i64> {
        self.entries.iter().map(|e| e.id).max()
    }

    fn seen(&self) -> AskSeen {
        AskSeen {
            ask_entry_count: i32::try_from(self.entries.len()).unwrap_or(i32::MAX),
            newest_ask_entry_id: self.newest_entry_id(),
        }
    }
}

async fn fetch_ledger_for_reauthoring(
    ph: &PersistHandle,
    session_uuid: Uuid,
) -> Option<LedgerForAuthoring> {
    match ph
        .fetch_ask_ledger_for_model(session_uuid, INTAKE_ASK_LEDGER_LIMIT)
        .await
    {
        Ok(rows) => {
            let fetched = rows.len();
            Some(LedgerForAuthoring {
                entries: written_by_the_operator(rows),
                fetched,
            })
        }
        Err(e) => {
            warn!(
                session_id = %session_uuid,
                error = %e,
                "intake criteria: ask-ledger fetch failed — no re-author this turn"
            );
            None
        }
    }
}

async fn fetch_seen_for_reauthoring(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: i64,
) -> Option<Option<AskSeen>> {
    match ph.fetch_acceptance_criteria_ask_seen(session_db_id).await {
        Ok(seen) => Some(seen),
        Err(e) => {
            warn!(
                session_id = %session_uuid,
                error = %e,
                "intake criteria: current-map lookup failed — no re-author this turn"
            );
            None
        }
    }
}

async fn count_older_asks_not_fetched(
    ph: &PersistHandle,
    session_uuid: Uuid,
    fetched: usize,
) -> Option<usize> {
    match ph.count_ask_ledger_for_model(session_uuid).await {
        Ok(total) => Some(usize::try_from(total).unwrap_or(0).saturating_sub(fetched)),
        Err(e) => {
            warn!(
                session_id = %session_uuid,
                error = %e,
                "intake criteria: ask-ledger count failed — no re-author this turn"
            );
            None
        }
    }
}

fn log_criteria_map_authored(
    session_uuid: Uuid,
    map_id: i64,
    criteria_len: usize,
    ask_entry_count: i32,
) {
    info!(
        session_id = %session_uuid,
        map_id,
        criteria = criteria_len,
        ask_entry_count,
        "intake criteria: authored a criteria map from the operator's words"
    );
}

fn log_criteria_map_store_failed(session_uuid: Uuid, e: &PersistError) {
    warn!(
        session_id = %session_uuid,
        error = %e,
        "intake criteria: authored map could not be stored"
    );
}

async fn store_authored_criteria(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: i64,
    seen: AskSeen,
    model: &str,
    criteria: Vec<NewCriterion>,
) {
    match ph
        .insert_acceptance_criteria_map(session_db_id, seen, Some(model), None, &criteria)
        .await
    {
        Ok(map_id) => {
            log_criteria_map_authored(session_uuid, map_id, criteria.len(), seen.ask_entry_count)
        }
        Err(e) => log_criteria_map_store_failed(session_uuid, &e),
    }
}

fn log_ask_unchanged_skip(session_uuid: Uuid, newest_ask_entry_id: Option<i64>) {
    debug!(
        session_id = %session_uuid,
        newest_ask_entry_id,
        "intake criteria: ask has not moved — no call"
    );
}

fn log_no_usable_key_for_reauthoring(session_uuid: Uuid) {
    debug!(
        session_id = %session_uuid,
        "intake criteria: no usable API key — session keeps whatever map it has"
    );
}

pub(super) async fn reauthor_intake_criteria(
    ctx: &FleetCtx,
    ph: &PersistHandle,
    session_uuid: Uuid,
) {
    let Some(session_db_id) = ph
        .resolve_session_db_id(&session_uuid.to_string())
        .await
        .ok()
        .flatten()
    else {
        return;
    };

    let Some(ledger) = fetch_ledger_for_reauthoring(ph, session_uuid).await else {
        return;
    };
    let Some(seen) = fetch_seen_for_reauthoring(ph, session_uuid, session_db_id).await else {
        return;
    };
    if !intake_criteria::should_reauthor(seen, ledger.newest_entry_id()) {
        log_ask_unchanged_skip(session_uuid, ledger.newest_entry_id());
        return;
    }

    let Some(llm) = crate::llm::key::effective_client(&ctx.llm, &ctx.key_slot) else {
        log_no_usable_key_for_reauthoring(session_uuid);
        return;
    };
    let Some(older_not_fetched) =
        count_older_asks_not_fetched(ph, session_uuid, ledger.fetched).await
    else {
        return;
    };
    let project_path = ph
        .fetch_session_project_path(session_db_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let budget = intake_criteria::IntakeInputBudget::from_setting(
        &ctx.settings.load().steerer_intake_criteria_input_budget,
    );
    let Some(criteria) = intake_criteria::author(
        &llm,
        &ledger.entries,
        older_not_fetched,
        &project_path,
        budget,
    )
    .await
    else {
        return;
    };
    let model = llm.model().to_owned();
    store_authored_criteria(
        ph,
        session_uuid,
        session_db_id,
        ledger.seen(),
        &model,
        criteria,
    )
    .await;
}
