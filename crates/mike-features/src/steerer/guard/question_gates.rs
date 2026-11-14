use serde_json::{Value, json};

use super::gates::{EndedOn, GateOutcome, PhraseGates};
use super::step_class::{Authorisation, DirectiveInForce, StepClass, authorise, classify_ending};
use super::{Question, Trigger};
use crate::dossier::steering_facts::SteeringFacts;
use crate::dossier::steering_facts::wait_backing::WaitBacking;

pub const REAL_DECISION_TOOLS: [&str; 2] = ["AskUserQuestion", "ExitPlanMode"];

#[derive(Debug, Clone, PartialEq)]
pub struct PriorReaction {
    pub turn_id: i64,
    pub reaction: Option<String>,
    pub verdict_id: Option<i64>,
    pub rationale: Option<String>,
}

pub struct GateContext<'a> {
    pub phrase: &'a PhraseGates,
    pub trigger: Trigger,
    pub facts: Option<&'a SteeringFacts>,
    pub prior: &'a [PriorReaction],
    pub open_turn_id: i64,
    pub turn_judge_live: bool,
    pub last_tool: Option<&'a str>,
    pub directives: &'a [DirectiveInForce],
    pub idle_seconds: Option<i64>,
    pub quiescence_after_secs: i64,
    pub ending: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestionGate {
    pub outcome: GateOutcome,
    pub relapse_anchor: Option<i64>,
    pub step_class: Option<StepClass>,
}

impl QuestionGate {
    fn plain(outcome: GateOutcome) -> Self {
        Self {
            outcome,
            relapse_anchor: None,
            step_class: None,
        }
    }
}

fn real_decision(ctx: &GateContext<'_>) -> Option<String> {
    ctx.last_tool
        .filter(|t| REAL_DECISION_TOOLS.contains(t))
        .map(|t| format!("real_decision:{t}"))
}

fn reaction_of(p: &PriorReaction) -> Option<&str> {
    p.reaction.as_deref()
}

pub fn relapse_gate(ctx: &GateContext<'_>) -> QuestionGate {
    let facts = json!({"prior": ctx.prior.iter().map(|p| json!({"turn_id": p.turn_id, "reaction": p.reaction, "verdict_id": p.verdict_id})).collect::<Vec<_>>()});
    if !ctx.turn_judge_live {
        return QuestionGate::plain(GateOutcome::closed("g1", "no_live_reaction", facts));
    }
    let p1 = ctx.prior.last();
    let p2 = ctx
        .prior
        .len()
        .checked_sub(2)
        .and_then(|i| ctx.prior.get(i));
    let open_on = |anchor: i64, corrected: &PriorReaction| QuestionGate {
        outcome: GateOutcome::open("g1", corrected.rationale.clone(), facts.clone()),
        relapse_anchor: Some(anchor),
        step_class: None,
    };
    if let Some(p) = p1.filter(|p| reaction_of(p) == Some("corrected")) {
        return open_on(ctx.open_turn_id, p);
    }
    if let (Some(first), Some(second)) = (p1, p2)
        && reaction_of(second) == Some("corrected")
        && reaction_of(first) != Some("accepted")
    {
        return open_on(first.turn_id, second);
    }
    let reason = if p1.is_some_and(|p| p.reaction.is_none()) {
        "reaction_pending"
    } else {
        "no_recent_correction"
    };
    QuestionGate::plain(GateOutcome::closed("g1", reason, facts))
}

fn backing_closed(arm: &str, facts: Option<&SteeringFacts>) -> Option<GateOutcome> {
    let Some(f) = facts else {
        return Some(GateOutcome::closed(
            arm,
            "backing_unknown",
            json!({"facts": null}),
        ));
    };
    let used = json!({"wait_backing": f.wait_backing.code(), "refs": f.wait_backing_refs});
    match f.wait_backing {
        WaitBacking::None => None,
        WaitBacking::Unknown => Some(GateOutcome::closed(arm, "backing_unknown", used)),
        other => Some(GateOutcome::closed(
            arm,
            format!("backed:{}", other.code()),
            used,
        )),
    }
}

fn phrase_arm(ctx: &GateContext<'_>) -> GateOutcome {
    let ended = ctx.phrase.ended_on;
    if matches!(ended, EndedOn::Handback | EndedOn::Question) {
        return GateOutcome::closed("g2", "question_ending", json!({"ended_on": ended.code()}));
    }
    if !ctx.phrase.g2 {
        return GateOutcome::closed("g2", "no_wait_ending", json!({"ended_on": ended.code()}));
    }
    backing_closed("g2", ctx.facts).unwrap_or_else(|| {
        GateOutcome::open(
            "g2",
            ctx.phrase.wait_match.clone(),
            json!({"wait_backing": "none", "ended_on": ended.code()}),
        )
    })
}

fn phrase_free_arm(ctx: &GateContext<'_>) -> GateOutcome {
    let ended = ctx.phrase.ended_on;
    if !matches!(ended, EndedOn::Other | EndedOn::Report) {
        return GateOutcome::closed("g2b", "phrase_ending", json!({"ended_on": ended.code()}));
    }
    let idle = ctx.idle_seconds.unwrap_or(0);
    if idle < ctx.quiescence_after_secs {
        return GateOutcome::closed("g2b", "not_quiet", json!({"idle_seconds": idle}));
    }
    backing_closed("g2b", ctx.facts).unwrap_or_else(|| {
        GateOutcome::open(
            "g2b",
            None,
            json!({"wait_backing": "none", "ended_on": ended.code(), "idle_seconds": idle}),
        )
    })
}

pub fn wait_gate(ctx: &GateContext<'_>) -> QuestionGate {
    if let Some(reason) = real_decision(ctx) {
        return QuestionGate::plain(GateOutcome::closed(
            "g2",
            reason,
            json!({"last_tool": ctx.last_tool}),
        ));
    }
    let g2 = phrase_arm(ctx);
    if ctx.trigger == Trigger::TurnEnd {
        let mut outcome = g2;
        outcome.arm = Some("g2");
        return QuestionGate::plain(outcome);
    }
    let g2b = phrase_free_arm(ctx);
    let (mut chosen, other, arm, other_arm) = if g2.open || !g2b.open {
        (g2, g2b, "g2", "g2b")
    } else {
        (g2b, g2, "g2b", "g2")
    };
    chosen.arm = Some(arm);
    let mut others = serde_json::Map::new();
    others.insert(
        other_arm.to_owned(),
        json!({"open": other.open, "reason": other.reason, "gate_id": other.gate_id}),
    );
    chosen.other_arms = Value::Object(others);
    QuestionGate::plain(chosen)
}

pub fn handback_gate(ctx: &GateContext<'_>) -> QuestionGate {
    if !ctx.phrase.g3 {
        return QuestionGate::plain(GateOutcome::closed("g3", "no_handback_ending", json!({})));
    }
    let Some(class) = ctx
        .phrase
        .step_class
        .or_else(|| classify_ending(ctx.ending))
    else {
        return QuestionGate::plain(GateOutcome::closed("g3", "no_step_class", json!({})));
    };
    let used = |extra: Value| json!({"step_class": class.code(), "authorisation": extra});
    let closed = |reason: String, extra: Value| QuestionGate {
        outcome: GateOutcome::closed("g3", reason, used(extra)),
        relapse_anchor: None,
        step_class: Some(class),
    };
    if class.is_boundary() {
        return closed(format!("real_decision:{}", class.code()), json!(null));
    }
    match authorise(class, ctx.directives) {
        Authorisation::Forbidden(ids) => closed("not_authorised:forbidden".to_owned(), json!(ids)),
        Authorisation::NotAuthorised => closed("not_authorised".to_owned(), json!([])),
        Authorisation::Authorised(ids) => QuestionGate {
            outcome: GateOutcome::open("g3", ctx.phrase.handback_match.clone(), used(json!(ids))),
            relapse_anchor: None,
            step_class: Some(class),
        },
    }
}

pub fn done_gate(ctx: &GateContext<'_>) -> QuestionGate {
    if let Some(reason) = real_decision(ctx) {
        return QuestionGate::plain(GateOutcome::closed(
            "g4",
            reason,
            json!({"last_tool": ctx.last_tool}),
        ));
    }
    if !ctx.phrase.g4 {
        return QuestionGate::plain(GateOutcome::closed("g4", "no_done_claim", json!({})));
    }
    let observation = ctx.facts.map(|f| &f.observation);
    let used = json!({"observation": observation});
    let outcome = match observation.and_then(|o| o.observed_after_change) {
        None => GateOutcome::closed("g4", "no_change", used),
        Some(true) => {
            let kind = observation
                .and_then(|o| o.observation_kind)
                .unwrap_or("unknown");
            GateOutcome::closed("g4", format!("observed:{kind}"), used)
        }
        Some(false) => GateOutcome::open("g4", ctx.phrase.done_match.clone(), used),
    };
    QuestionGate::plain(outcome)
}

pub fn gate(question: Question, ctx: &GateContext<'_>) -> QuestionGate {
    match question {
        Question::Relapse => relapse_gate(ctx),
        Question::UnbackedWait => wait_gate(ctx),
        Question::AuthorisedHandback => handback_gate(ctx),
        Question::UnobservedDone => done_gate(ctx),
    }
}
