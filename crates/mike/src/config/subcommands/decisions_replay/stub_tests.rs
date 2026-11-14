use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use super::correction;
use super::jev::{Jev, Reply};
use super::turn_judge;
use super::verifier;
use super::wake;
use crate::llm::decisions::{Answer, DecisionRequest};
use crate::store::persistence::decisions_replay::{
    CorrectionItem, TurnJudgeItem, VerifierItem, WakeItem,
};
use crate::store::persistence::turn_verdicts::JudgeTurnContextRow;

pub(super) enum Mode {
    Answer(f64),
    Pick(&'static [(&'static str, f64)]),
    Fail,
    Stop,
}

pub(super) struct Stub {
    pub mode: Mode,
    pub calls: AtomicUsize,
}

impl Stub {
    pub(super) fn new(mode: Mode) -> Self {
        Self {
            mode,
            calls: AtomicUsize::new(0),
        }
    }

    pub(super) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Jev for Stub {
    fn model(&self) -> &str {
        "stub-jev"
    }

    async fn ask(&self, request: &DecisionRequest) -> Reply {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            Mode::Answer(noul) => answered(request, |_| noul),
            Mode::Pick(picks) => answered(request, |q| {
                picks
                    .iter()
                    .find(|(name, _)| *name == q)
                    .map_or(0.5, |(_, p)| *p)
            }),
            Mode::Fail => Reply::Failed("timeout".to_owned()),
            Mode::Stop => Reply::Stop("daily ceiling".to_owned()),
        }
    }
}

fn answered(request: &DecisionRequest, noul: impl Fn(&str) -> f64) -> Reply {
    Reply::Answered {
        model: "stub-jev-served".to_owned(),
        answers: request
            .questions
            .keys()
            .map(|q| {
                (
                    q.clone(),
                    Answer::Noul {
                        noul: noul(q),
                        confidence: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
        cost_usd: Some(0.001),
        latency_ms: Some(120),
    }
}

pub(super) fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 1, 9, 0, 0)
        .single()
        .unwrap_or_default()
}

pub(super) fn correction_ready(id: i64, kind: &str, keyword: Option<&str>) -> correction::Ready {
    correction::Ready {
        item: CorrectionItem {
            entry_id: id,
            session_id: 1,
            request_id: id + 100,
            kind: kind.to_owned(),
            keyword_kind: keyword.map(str::to_owned),
            content: "that is not what I asked".to_owned(),
            created_at: at(),
        },
        prior_turn: "Done, the parser is fixed.".to_owned(),
    }
}

pub(super) fn wake_ready(label: &str) -> wake::Ready {
    wake::Ready {
        item: WakeItem {
            evaluation_uuid: Uuid::nil(),
            intervention_uuid: Uuid::from_u128(7),
            session_id: 1,
            operator_turn_id: 2,
            question: Some("unbacked_wait".to_owned()),
            label: label.to_owned(),
            label_source: "operator".to_owned(),
            wake_state: Some("pushed".to_owned()),
            nudge: "You said you would wait for the build.".to_owned(),
            decided_at: at(),
        },
        facts: None,
    }
}

pub(super) fn verifier_ready(
    label: &str,
    verdict_json: Option<serde_json::Value>,
) -> verifier::Ready {
    let item = VerifierItem {
        evaluation_uuid: Uuid::nil(),
        intervention_uuid: Uuid::from_u128(9),
        session_id: 1,
        operator_turn_id: 2,
        trigger_request_id: None,
        label: label.to_owned(),
        label_source: "session_reply".to_owned(),
        nudge: "You did not run the tests.".to_owned(),
        verdict_json,
        operator_ask: "fix the parser and test it".to_owned(),
        turn_end: "Fixed the parser.".to_owned(),
        dispatched_at: at(),
    };
    let verdict = verifier::verdict_of(&item);
    verifier::Ready { item, verdict }
}

pub(super) fn turn_judge_ready(outcome: &str, next: Option<&str>) -> turn_judge::Ready {
    turn_judge::Ready {
        item: TurnJudgeItem {
            turn_id: 5,
            session_id: 1,
            closed_at: at(),
            outcome: outcome.to_owned(),
            gold_reaction: Some("corrected".to_owned()),
            gold_delivery: Some("delivered".to_owned()),
            live_reaction: Some("accepted".to_owned()),
            live_delivery: Some("delivered".to_owned()),
            live_legacy_label: None,
            live_confidence: Some(0.8),
        },
        context: JudgeTurnContextRow {
            turn_id: 5,
            session_id: 1,
            user_prompt: "fix the parser".to_owned(),
            final_assistant_text: "Fixed it and ran the tests.".to_owned(),
            tool_call_count: 4,
            tool_error_count: 0,
            closed_at: at(),
            next_state: "resolved".to_owned(),
            next_genuine_request_id: next.map(|_| 9),
            next_genuine_text: next.map(str::to_owned),
            next_genuine_at: None,
            text_cut_at: 0,
            prompt_kind: "genuine".to_owned(),
            session_class: "work".to_owned(),
            stop_reason: None,
            aborted: false,
            checks_count: 0,
            checks_failed: 0,
            checks_run: serde_json::Value::Null,
            ended_on: "report".to_owned(),
            files_changed: 1,
        },
    }
}
