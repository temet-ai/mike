use super::EvalWindow;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::steerer::note_gate::{self, NoteInput, NoteKind};
use crate::steerer::registry::Steerer;
use crate::steerer::rule_check::TurnTools;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::steering_facts::FactKey;

async fn call_lines(persist: &PersistHandle, session_id: i64, turn_id: i64) -> Vec<String> {
    let key = FactKey {
        session_id,
        turn_id,
        as_of: chrono::Utc::now(),
    };
    let rows = persist
        .dossier()
        .steering_call_rows(&[key])
        .await
        .unwrap_or_default();
    TurnTools::from_calls(rows.iter().map(|c| (c.tool_name.as_str(), &c.input))).lines
}

pub async fn verifier_admits(window: &EvalWindow<'_>, steerer: &Steerer, nudge: &str) -> bool {
    if steerer.kind != "verifier" {
        return true;
    }
    let Some(persist) = window.ctx.persist.as_ref() else {
        return true;
    };
    let Some(decider) = Decider::for_point(
        window.base_llm,
        persist,
        window.ctx.toggles.load().decisions,
        &window.ctx.settings.load(),
        note_gate::POINT,
    ) else {
        return true;
    };
    let scope = window.scope();
    let lines = match scope {
        Some(s) => call_lines(persist, s.session_db_id, s.operator_turn_id).await,
        None => Vec::new(),
    };
    let input = NoteInput {
        kind: NoteKind::Ask,
        note: nudge,
        operator_ask: &window.eval_inputs.operator_ask,
        tool_lines: &lines,
        final_text: &window.eval_inputs.eval_text,
    };
    let subject = DecisionSubject {
        session_id: scope.map(|s| s.session_db_id),
        operator_turn_id: scope.map(|s| s.operator_turn_id),
        request_id: window.target.trigger_request_id,
        ..DecisionSubject::default()
    };
    note_gate::admits(Some(&decider), &input, subject).await
}
