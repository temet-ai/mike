use chrono::{DateTime, Utc};

use super::gates::{PhraseGates, phrase_gates};
use super::question_gates::PriorReaction;
use super::step_class::DirectiveInForce;
use crate::dossier::steering_facts::SteeringFacts;
use crate::dossier::view::{AsOf, DossierError, DossierInputs, Needs};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::dossier_view::OpenTextRow;

pub struct TurnContext {
    pub inputs: DossierInputs,
    pub facts: Option<SteeringFacts>,
    pub last_tool: Option<String>,
    pub ending: String,
    pub phrase: PhraseGates,
    pub prior: Vec<PriorReaction>,
    pub directives: Vec<DirectiveInForce>,
    pub open_prompt: Option<String>,
    pub opened_by: Option<String>,
}

pub struct ContextRequest<'a> {
    pub persist: &'a PersistHandle,
    pub session_db_id: i64,
    pub turn_id: i64,
    pub as_of: AsOf,
    pub fallback_text: &'a str,
    pub live_request_id: Option<i64>,
    pub verifier_pattern: &'a str,
}

pub(crate) fn turn_ending(
    rows: &[OpenTextRow],
    live_request_id: Option<i64>,
    live_text: &str,
) -> String {
    let live = live_request_id.filter(|_| !live_text.trim().is_empty());
    let mut text: String = rows
        .iter()
        .filter(|r| !r.origin.contains("sidecar") && Some(r.request_id) != live)
        .map(|r| r.content.as_str())
        .collect();
    if live.is_some() {
        text.push_str(live_text);
    }
    if text.trim().is_empty() {
        live_text.to_owned()
    } else {
        text
    }
}

pub fn prior_reactions(inputs: &DossierInputs, turn_id: i64) -> Vec<PriorReaction> {
    let mut prior: Vec<&crate::store::persistence::dossier_view::DossierTurnRow> = inputs
        .turns
        .iter()
        .filter(|t| t.turn_id < turn_id && t.opened_by.as_deref().is_none_or(|o| o == "operator"))
        .collect();
    prior.sort_by_key(|t| (t.created_at, t.turn_id));
    prior
        .iter()
        .rev()
        .take(2)
        .rev()
        .map(|t| {
            let latest = inputs
                .reactions
                .iter()
                .filter(|r| r.turn_id == t.turn_id)
                .max_by_key(|r| (r.created_at, r.verdict_id));
            PriorReaction {
                turn_id: t.turn_id,
                reaction: latest.map(|r| r.reaction.clone()),
                verdict_id: latest.map(|r| r.verdict_id),
                rationale: latest.and_then(|r| r.rationale.clone()),
            }
        })
        .collect()
}

pub fn directives_in_force(inputs: &DossierInputs, at: DateTime<Utc>) -> Vec<DirectiveInForce> {
    inputs
        .entries
        .iter()
        .filter(|e| {
            e.kind == "directive" && e.effective_at <= at && e.closed_at.is_none_or(|c| c > at)
        })
        .filter_map(|e| {
            let class = e.data.get("class")?.as_str()?.to_owned();
            let polarity = e.data.get("polarity")?.as_str()?.to_owned();
            let text = e
                .operator_turn_id
                .and_then(|id| inputs.turns.iter().find(|t| t.turn_id == id))
                .map_or_else(|| e.source_ref.clone(), |t| t.prompt.clone());
            Some(DirectiveInForce {
                id: e.id,
                class,
                polarity,
                text,
                operator_turn_id: e.operator_turn_id,
            })
        })
        .collect()
}

pub async fn load_context(req: &ContextRequest<'_>) -> Result<TurnContext, DossierError> {
    let needs = Needs {
        open_turn: Some(req.turn_id),
        steering_facts: Some(req.turn_id),
        ..Needs::everything(None)
    };
    let mut inputs =
        crate::dossier::view::load(req.persist, req.session_db_id, req.as_of, needs).await?;
    let facts = inputs.steering_facts.take();
    let last_tool = req
        .persist
        .last_tool_of_turn(req.turn_id)
        .await
        .ok()
        .flatten();
    let rows = inputs
        .open_turn
        .as_ref()
        .map_or(&[][..], |(_, r)| r.as_slice());
    let ending = turn_ending(rows, req.live_request_id, req.fallback_text);
    let phrase = phrase_gates(&ending, req.verifier_pattern);
    let open = inputs.turns.iter().find(|t| t.turn_id == req.turn_id);
    let open_prompt = open.map(|t| t.prompt.clone());
    let opened_by = open.and_then(|t| t.opened_by.clone());
    Ok(TurnContext {
        prior: prior_reactions(&inputs, req.turn_id),
        directives: directives_in_force(&inputs, req.as_of.at),
        facts,
        last_tool,
        ending,
        phrase,
        open_prompt,
        opened_by,
        inputs,
    })
}

#[cfg(test)]
#[path = "tests/context_tests.rs"]
mod context_tests;
