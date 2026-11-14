use super::EvalWindow;
use super::jev_note::{JevNote, deliver};
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::steerer::commit_message as commit_point;
use crate::steerer::guard::GuardTurn;
use crate::steerer::note_policy::{self, NoteLimits, SessionNotes};
use crate::steerer::registry::Steerer;
use crate::steerer::rule_check as rule_point;
use crate::steerer::verdict::Need;
use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::steering_facts::{CallRow, FactKey};

struct TurnScope<'a> {
    turn: &'a GuardTurn<'a>,
    checkout: String,
    calls: Vec<CallRow>,
}

fn decider_for(window: &EvalWindow<'_>, point: &'static str) -> Option<Decider> {
    let persist = window.ctx.persist.as_ref()?;
    Decider::for_point(
        window.base_llm,
        persist,
        window.ctx.toggles.load().decisions,
        &window.ctx.settings.load(),
        point,
    )
}

fn subject(turn: &GuardTurn<'_>) -> DecisionSubject {
    DecisionSubject {
        session_id: Some(turn.session_db_id),
        operator_turn_id: Some(turn.operator_turn_id),
        request_id: turn.trigger_request_id,
        ..DecisionSubject::default()
    }
}

async fn scope<'a>(turn: &'a GuardTurn<'a>) -> Option<TurnScope<'a>> {
    let uuid = turn.session_uuid?;
    let key = FactKey {
        session_id: turn.session_db_id,
        turn_id: turn.operator_turn_id,
        as_of: turn.now,
    };
    let calls = turn
        .persist
        .dossier()
        .steering_call_rows(&[key])
        .await
        .ok()?;
    if calls.is_empty() {
        return None;
    }
    let checkout = turn
        .persist
        .session_identity_by_uuid(uuid)
        .await
        .ok()??
        .cwd?;
    Some(TurnScope {
        turn,
        checkout,
        calls,
    })
}

async fn admitted(window: &EvalWindow<'_>, steerer: &Steerer, turn: &GuardTurn<'_>) -> bool {
    let limits = NoteLimits::from_settings(&window.ctx.settings.load());
    let notes = SessionNotes::load(turn.persist, turn.session_db_id, limits, turn.now).await;
    note_policy::refusal(Some(&notes), steerer.id, Need::Medium).is_none()
}

async fn commit_note(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    scope: &TurnScope<'_>,
    decider: &Decider,
) -> bool {
    let now = scope.turn.now.timestamp();
    let mut commit = None;
    for call in scope.calls.iter().rev().filter(|c| c.has_result) {
        let command = call.input.get("command").and_then(|c| c.as_str());
        let Some(command) =
            command.filter(|c| commit_point::git::is_commit_command(&call.tool_name, c))
        else {
            continue;
        };
        commit = commit_point::git::read(&scope.checkout, command, now).await;
        break;
    }
    let Some(commit) = commit else {
        return false;
    };
    let Some(checked) = commit_point::check(decider, &commit, subject(scope.turn)).await else {
        return false;
    };
    let text = checked.warning.note(&commit);
    let note = JevNote {
        topic: commit_point::POINT,
        text: &text,
        operator_turn_id: scope.turn.operator_turn_id,
    };
    let delivered = checked.warning.warn && deliver(window, steerer, &note).await.is_some();
    commit_point::mark(decider, &checked, delivered).await;
    delivered
}

async fn rule_note(
    window: &EvalWindow<'_>,
    steerer: &Steerer,
    scope: &TurnScope<'_>,
    decider: &Decider,
) {
    let rules = rule_point::rules::load(&scope.checkout).await;
    let tools = rule_point::TurnTools::from_calls(
        scope.calls.iter().map(|c| (c.tool_name.as_str(), &c.input)),
    );
    let picked = rule_point::rules::relevant(&rules, &tools, rule_point::RULES_PER_TURN);
    let input = rule_point::TurnInput {
        operator_ask: &window.eval_inputs.operator_ask,
        turn_end: &scope.turn.text,
        tools: &tools,
    };
    let Some(checked) = rule_point::check(decider, &input, &picked, subject(scope.turn)).await
    else {
        return;
    };
    let text = checked.outcome.note(checked.rule);
    let note = JevNote {
        topic: rule_point::POINT,
        text: &text,
        operator_turn_id: scope.turn.operator_turn_id,
    };
    let delivered = checked.outcome.violated && deliver(window, steerer, &note).await.is_some();
    rule_point::mark(decider, &checked, delivered).await;
}

pub async fn after_turn(window: &EvalWindow<'_>, steerer: &Steerer, turn: &GuardTurn<'_>) {
    let commit_decider = decider_for(window, commit_point::POINT);
    let rule_decider = decider_for(window, rule_point::POINT);
    if commit_decider.is_none() && rule_decider.is_none() {
        return;
    }
    if window.ctx.config.force_fire || !admitted(window, steerer, turn).await {
        return;
    }
    let Some(scope) = scope(turn).await else {
        return;
    };
    if let Some(decider) = commit_decider.as_ref()
        && commit_note(window, steerer, &scope, decider).await
    {
        return;
    }
    if let Some(decider) = rule_decider.as_ref() {
        rule_note(window, steerer, &scope, decider).await;
    }
}
