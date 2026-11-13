use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::call::{CallRequest, SYSTEM_PROMPT, ask_with_quorum, decide};
use super::context::{ContextRequest, TurnContext, load_context};
use super::exempt::{ExemptInput, exemption};
use super::gates::phrase_gates;
use super::input::{InputParts, build_input};
use super::metrics::{Interval, wilson};
use super::policy::GuardConfig;
use super::question_gates::{GateContext, gate};
use super::{Question, Trigger};
use crate::dossier::view::AsOf;
use crate::llm::OpenRouterClient;
use crate::store::persistence::steering::guard::{GuardReplayTurn, ReplayWindow};
use crate::store::persistence::{PersistError, PersistHandle};

pub const REPLAY_BATCH: i64 = 500;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GatesReport {
    pub turns: usize,
    pub exempt: BTreeMap<String, usize>,
    pub phrase_g2: usize,
    pub phrase_g3: usize,
    pub phrase_g4: usize,
    pub ended_on: BTreeMap<String, usize>,
    pub open: BTreeMap<String, usize>,
    pub closed: BTreeMap<String, BTreeMap<String, usize>>,
    pub open_turns: BTreeMap<String, Vec<i64>>,
}

pub struct ReplayRequest<'a> {
    pub persist: &'a PersistHandle,
    pub window: ReplayWindow,
    pub verifier_pattern: &'a str,
    pub turn_judge_live: bool,
    pub config: GuardConfig,
    pub calibration: Option<crate::cross_session::CrossSessionSnapshot>,
}

fn gate_ctx<'a>(
    ctx: &'a TurnContext,
    turn: &GuardReplayTurn,
    req: &ReplayRequest<'_>,
    trigger: Trigger,
) -> GateContext<'a> {
    GateContext {
        phrase: &ctx.phrase,
        trigger,
        facts: ctx.facts.as_ref(),
        prior: &ctx.prior,
        open_turn_id: turn.turn_id,
        turn_judge_live: req.turn_judge_live,
        last_tool: ctx.last_tool.as_deref(),
        directives: &ctx.directives,
        idle_seconds: Some(req.config.quiescence_after_secs),
        quiescence_after_secs: req.config.quiescence_after_secs,
        ending: &ctx.ending,
    }
}

async fn turn_context(req: &ReplayRequest<'_>, turn: &GuardReplayTurn) -> Option<TurnContext> {
    let at = turn.closed_at?;
    load_context(&ContextRequest {
        persist: req.persist,
        session_db_id: turn.session_id,
        turn_id: turn.turn_id,
        as_of: AsOf::reconstructed_for_eval(at),
        fallback_text: &turn.tail,
        live_request_id: None,
        verifier_pattern: req.verifier_pattern,
    })
    .await
    .ok()
}

fn exempt_of(ctx: &TurnContext, turn: &GuardReplayTurn) -> Option<String> {
    exemption(&ExemptInput {
        agent_id: turn.agent_id.as_deref(),
        session_class: ctx
            .facts
            .as_ref()
            .and_then(|f| f.session_class.as_ref())
            .map(|c| c.class),
        opened_by: turn.opened_by.as_deref(),
        open_prompt: turn.user_prompt.as_deref(),
    })
}

fn tally_phrase(report: &mut GatesReport, text: &str, verifier_pattern: &str) {
    let phrase = phrase_gates(text, verifier_pattern);
    report.phrase_g2 += usize::from(phrase.g2);
    report.phrase_g3 += usize::from(phrase.g3);
    report.phrase_g4 += usize::from(phrase.g4);
    *report
        .ended_on
        .entry(phrase.ended_on.code().to_owned())
        .or_default() += 1;
}

fn tally(
    report: &mut GatesReport,
    ctx: &TurnContext,
    turn: &GuardReplayTurn,
    req: &ReplayRequest<'_>,
) {
    for trigger in [Trigger::TurnEnd, Trigger::Quiescence] {
        let g = gate_ctx(ctx, turn, req, trigger);
        let questions: &[Question] = if trigger == Trigger::TurnEnd {
            &Question::ALL
        } else {
            &[Question::UnbackedWait]
        };
        for q in questions {
            let outcome = gate(*q, &g).outcome;
            let key = format!(
                "{}:{}:{}",
                q.code(),
                trigger.code(),
                outcome.arm.unwrap_or("-")
            );
            if outcome.open {
                *report.open.entry(key.clone()).or_default() += 1;
                report.open_turns.entry(key).or_default().push(turn.turn_id);
            } else {
                *report
                    .closed
                    .entry(key)
                    .or_default()
                    .entry(outcome.reason)
                    .or_default() += 1;
            }
        }
    }
}

pub async fn replay_gates(req: &ReplayRequest<'_>) -> Result<GatesReport, PersistError> {
    let mut report = GatesReport::default();
    let mut window = req.window.clone();
    loop {
        let turns = req.persist.steerer().guard_replay_turns(&window).await?;
        let Some(last) = turns.last().map(|t| t.turn_id) else {
            break;
        };
        for turn in &turns {
            report.turns += 1;
            tally_phrase(&mut report, &turn.tail, req.verifier_pattern);
            let Some(ctx) = turn_context(req, turn).await else {
                *report
                    .exempt
                    .entry("context_unavailable".to_owned())
                    .or_default() += 1;
                continue;
            };
            if let Some(reason) = exempt_of(&ctx, turn) {
                *report.exempt.entry(reason).or_default() += 1;
                continue;
            }
            tally(&mut report, &ctx, turn, req);
        }
        window.after_turn = last;
        if !window.turn_ids.is_empty() || turns.len() < usize::try_from(window.limit).unwrap_or(0) {
            break;
        }
    }
    Ok(report)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TruthLabel {
    pub turn_id: i64,
    pub question: String,
    pub positive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct QuestionTally {
    pub asked: usize,
    pub would_fire: usize,
    pub tp: usize,
    pub fp: usize,
    pub false_negatives: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FullReplayReport {
    pub turns: usize,
    pub questions: BTreeMap<String, QuestionTally>,
    pub precision: BTreeMap<String, Interval>,
    pub exempt: BTreeMap<String, usize>,
    pub audit_set_sessions: usize,
    pub audit_set_fires: usize,
    pub audit_fire_turns: Vec<i64>,
    pub questions_with_calibration: BTreeMap<String, QuestionTally>,
    pub precision_with_calibration: BTreeMap<String, Interval>,
}

#[derive(Default)]
struct Tallies {
    plain: BTreeMap<String, QuestionTally>,
    with_block: BTreeMap<String, QuestionTally>,
}

fn tally_votes(
    votes: &[super::schema::GuardVerdict],
    open: &[Question],
    req: &ReplayRequest<'_>,
    positive: &dyn Fn(Question) -> Option<bool>,
    tallies: &mut BTreeMap<String, QuestionTally>,
) -> bool {
    let mut any = false;
    for q in open {
        let d = decide(*q, votes, req.config.threshold(*q));
        let t = tallies.entry(q.code().to_owned()).or_default();
        t.asked += 1;
        let truth = positive(*q);
        if d.fires {
            any = true;
            t.would_fire += 1;
            match truth {
                Some(true) => t.tp += 1,
                Some(false) => t.fp += 1,
                None => {}
            }
        } else if truth == Some(true) {
            t.false_negatives += 1;
        }
    }
    any
}

async fn replay_turn(
    llm: &OpenRouterClient,
    req: &ReplayRequest<'_>,
    ctx: &TurnContext,
    turn: &GuardReplayTurn,
    truth: &HashMap<(i64, String), bool>,
    tallies: &mut Tallies,
) -> bool {
    let g = gate_ctx(ctx, turn, req, Trigger::TurnEnd);
    let open: Vec<Question> = Question::ALL
        .into_iter()
        .filter(|q| gate(*q, &g).outcome.open)
        .collect();
    if open.is_empty() {
        return false;
    }
    let input = build_input(&InputParts {
        ending: &ctx.ending,
        open: open.clone(),
        directives: &ctx.directives,
        facts: ctx.facts.as_ref(),
        goal: ctx.inputs.goals.as_ref().map(|x| x.goal.as_str()),
        ..InputParts::default()
    });
    let thresholds = |q: Question| req.config.threshold(q);
    let positive = |q: Question| truth.get(&(turn.turn_id, q.code().to_owned())).copied();
    let plain = CallRequest {
        system: SYSTEM_PROMPT,
        input: &input,
        open: &open,
    };
    let Ok(votes) = ask_with_quorum(llm, &plain, &thresholds).await else {
        return false;
    };
    let any = tally_votes(&votes, &open, req, &positive, &mut tallies.plain);
    if let Some(snapshot) = &req.calibration {
        let with: Vec<(Question, f64)> = open
            .iter()
            .map(|q| (*q, req.config.threshold(*q)))
            .collect();
        let block = super::calibration_input::block_for(
            snapshot,
            &with,
            None,
            turn.closed_at.unwrap_or_else(chrono::Utc::now),
        );
        let system = super::calibration_input::system_prompt(SYSTEM_PROMPT, block.as_ref());
        let input_block = super::calibration_input::with_block(&input, block.as_ref());
        let calibrated = CallRequest {
            system: &system,
            input: &input_block,
            open: &open,
        };
        if let Ok(votes) = ask_with_quorum(llm, &calibrated, &thresholds).await {
            tally_votes(&votes, &open, req, &positive, &mut tallies.with_block);
        }
    }
    any
}

pub async fn replay_full(
    llm: &OpenRouterClient,
    req: &ReplayRequest<'_>,
    labels: &[TruthLabel],
) -> Result<FullReplayReport, PersistError> {
    let truth: HashMap<(i64, String), bool> = labels
        .iter()
        .map(|l| ((l.turn_id, l.question.clone()), l.positive))
        .collect();
    let turns = req
        .persist
        .steerer()
        .guard_replay_turns(&req.window)
        .await?;
    let mut tallies = Tallies::default();
    let mut exempt: BTreeMap<String, usize> = BTreeMap::new();
    let mut fired: Vec<(i64, i64)> = Vec::new();
    for turn in &turns {
        let Some(ctx) = turn_context(req, turn).await else {
            continue;
        };
        if let Some(reason) = exempt_of(&ctx, turn) {
            *exempt.entry(reason).or_default() += 1;
            continue;
        }
        if replay_turn(llm, req, &ctx, turn, &truth, &mut tallies).await {
            fired.push((turn.session_id, turn.turn_id));
        }
    }
    let audit_sessions = req
        .persist
        .steerer()
        .guard_audit_sessions(req.window.since)
        .await?;
    let audit_fire_turns: Vec<i64> = fired
        .iter()
        .filter(|(s, _)| audit_sessions.contains(s))
        .map(|(_, t)| *t)
        .collect();
    let precision_of = |t: &BTreeMap<String, QuestionTally>| -> BTreeMap<String, Interval> {
        t.iter()
            .map(|(q, t)| (q.clone(), wilson(t.tp as f64, (t.tp + t.fp) as f64)))
            .collect()
    };
    let precision = precision_of(&tallies.plain);
    let precision_with_calibration = precision_of(&tallies.with_block);
    Ok(FullReplayReport {
        turns: turns.len(),
        questions: tallies.plain,
        questions_with_calibration: tallies.with_block,
        precision_with_calibration,
        precision,
        exempt,
        audit_set_sessions: audit_sessions.len(),
        audit_set_fires: audit_fire_turns.len(),
        audit_fire_turns,
    })
}

#[cfg(test)]
mod tests {
    use super::{GatesReport, tally_phrase};

    #[test]
    fn phrase_census_tallies_complete_captured_tails() {
        let mut report = GatesReport::default();
        tally_phrase(&mut report, "I'll report back when it finishes.", "");
        tally_phrase(&mut report, "Want me to deploy?", "");
        tally_phrase(&mut report, "Implemented and verified.", "");
        assert_eq!(report.phrase_g2, 1);
        assert_eq!(report.phrase_g3, 1);
        assert_eq!(report.phrase_g4, 1);
        assert_eq!(report.ended_on["promise_or_wait"], 1);
        assert_eq!(report.ended_on["handback"], 1);
        assert_eq!(report.ended_on["report"], 1);
    }
}
