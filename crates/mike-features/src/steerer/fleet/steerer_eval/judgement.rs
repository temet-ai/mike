use serde_json::{Value, json};
use tracing::warn;

use crate::steerer::note_policy::NoteRefusal;
use crate::steerer::verdict::SteererVerdict;
use crate::store::persistence::steering_evaluations::EvaluationVerdict;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abstention {
    NoApiKey,
    ShutDown,
    UnknownInvocation,
    PatternGateDeclined,
    Debounced,
    AlreadyFiredForTurn,
    ArmingSuppressed,
    GovernorRefused,
    LlmCallFailed,
    VerdictDeclined,
    ConfidenceBelowThreshold,
    QuorumNotReached,
    MachineText,
    InvalidSessionId,
    EnrolmentLookupFailed,
    NoTurnClosedSteerer,
    DebouncedByTime,
    DebouncedByAdvice,
    Unactionable,
    RepeatsLastDelivery,
    Note(NoteRefusal),
}

impl Abstention {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NoApiKey => "no_api_key",
            Self::ShutDown => "shut_down",
            Self::UnknownInvocation => "unknown_invocation",
            Self::PatternGateDeclined => "pattern_gate_declined",
            Self::Debounced => "debounced",
            Self::AlreadyFiredForTurn => "already_fired_for_turn",
            Self::ArmingSuppressed => "arming_suppressed",
            Self::GovernorRefused => "governor_refused",
            Self::LlmCallFailed => "llm_call_failed",
            Self::VerdictDeclined => "verdict_declined",
            Self::ConfidenceBelowThreshold => "confidence_below_threshold",
            Self::QuorumNotReached => "quorum_not_reached",
            Self::MachineText => "machine_text",
            Self::InvalidSessionId => "invalid_session_id",
            Self::EnrolmentLookupFailed => "enrolment_lookup_failed",
            Self::NoTurnClosedSteerer => "no_turn_closed_steerer",
            Self::DebouncedByTime => "debounced_by_time",
            Self::DebouncedByAdvice | Self::RepeatsLastDelivery => "debounced_by_advice",
            Self::Unactionable => "unactionable",
            Self::Note(refusal) => refusal.code(),
        }
    }

    pub const fn explanation(self) -> &'static str {
        match self {
            Self::NoApiKey => "not judged: no OpenRouter API key is configured",
            Self::ShutDown => "not judged: the steerer fleet was shutting down",
            Self::UnknownInvocation => "not judged: the steerer has an unknown invocation mode",
            Self::PatternGateDeclined => {
                "not judged: neither the steerer pattern nor the done-claim wording matched"
            }
            Self::Debounced => "not judged: debounced after a recent intervention",
            Self::AlreadyFiredForTurn => "not judged: the steerer already fired for this turn",
            Self::ArmingSuppressed => "not judged: risk-based arming suppressed the verifier",
            Self::GovernorRefused => "not judged: the LLM call governor refused the call",
            Self::LlmCallFailed => "not judged: the LLM verdict call failed",
            Self::VerdictDeclined => "judged: the verdict declined to fire",
            Self::ConfidenceBelowThreshold => {
                "judged: the verdict wanted to fire but its confidence was below the threshold"
            }
            Self::QuorumNotReached => {
                "judged: the verdict wanted to fire but the 2-of-3 quorum was not reached"
            }
            Self::MachineText => "not judged: the turn opened with harness-generated text",
            Self::InvalidSessionId => "not judged: the session id is not a valid uuid",
            Self::EnrolmentLookupFailed => "not judged: the enrolment lookup failed",
            Self::NoTurnClosedSteerer => {
                "not judged: no enrolled steerer is triggered at turn close"
            }
            Self::DebouncedByTime => {
                "not judged: the steerer fired in this session within its minimum interval"
            }
            Self::DebouncedByAdvice => {
                "not judged: this session already received this steerer's advice"
            }
            Self::RepeatsLastDelivery => {
                "judged: the note would repeat the last one delivered to this session"
            }
            Self::Unactionable => "not judged: the advice needs a tool that is not available here",
            Self::Note(NoteRefusal::SessionNoteLimit) => {
                "not judged: this session already received its limit of notes for now"
            }
            Self::Note(NoteRefusal::QuietAfterWrongNote) => {
                "not judged: this steerer's last note to this session was judged wrong"
            }
            Self::Note(NoteRefusal::NeedBelowLevel) => {
                "judged: the note was not needed enough for this session right now"
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TurnJudgement {
    pub verdict: Option<SteererVerdict>,
    pub abstention: Option<Abstention>,
}

impl TurnJudgement {
    pub fn not_judged(reason: Abstention) -> Self {
        Self {
            verdict: None,
            abstention: Some(reason),
        }
    }

    pub fn evaluation_record(
        &self,
        fired: bool,
        retain_verdict_text: bool,
    ) -> Option<EvaluationVerdict> {
        match (&self.verdict, self.abstention) {
            (Some(verdict), abstention) => Some(judged_record(
                verdict,
                abstention,
                fired || retain_verdict_text,
            )),
            (None, Some(reason)) => Some(not_judged_record(reason)),
            (None, None) => None,
        }
    }
}

fn with_abstention(mut record: Value, abstention: Option<Abstention>) -> Value {
    if let (Some(reason), Value::Object(map)) = (abstention, &mut record) {
        map.insert("abstention".to_owned(), json!(reason.code()));
    }
    record
}

fn judged_record(
    verdict: &SteererVerdict,
    abstention: Option<Abstention>,
    keep_verdict_text: bool,
) -> EvaluationVerdict {
    if !keep_verdict_text {
        return EvaluationVerdict {
            verdict_json: with_abstention(json!({"judged": true}), abstention),
            confidence: None,
            reason: abstention.map(|reason| reason.explanation().to_owned()),
        };
    }
    let mut verdict_json = match serde_json::to_value(verdict) {
        Ok(value) => value,
        Err(e) => {
            warn!(
                error = %e,
                "steerer fleet: verdict could not be serialised for retention — recording the judgement without it"
            );
            json!({"fires": verdict.fires})
        }
    };
    if let Value::Object(map) = &mut verdict_json {
        map.insert("judged".to_owned(), json!(true));
    }
    EvaluationVerdict {
        verdict_json: with_abstention(verdict_json, abstention),
        confidence: Some(verdict.confidence),
        reason: Some(verdict.reason.clone()),
    }
}

fn not_judged_record(reason: Abstention) -> EvaluationVerdict {
    EvaluationVerdict {
        verdict_json: json!({"judged": false, "abstention": reason.code()}),
        confidence: None,
        reason: Some(reason.explanation().to_owned()),
    }
}
