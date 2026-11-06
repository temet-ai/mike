use chrono::{DateTime, Duration, Utc};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::ReplayItem;
use super::as_of::assemble;
use super::run::ReplaySteerer;
use crate::dossier::render::steerer::RECENT_REACTIONS_HEADING;
use crate::dossier::render::steerer_notes::{
    EARLIER_NOTES_HEADING, EARLIER_TURNS_HEADING, TURN_OPENING_HEADING,
};
use crate::steerer::fleet::input_budget::VerifierInputBudget;
use crate::store::persistence::PersistHandle;

const SECTION_PREFIXES: &[(&str, &str)] = &[
    ("user_ask", "User ask:"),
    ("latest_message", "Latest message:"),
    ("acting_agent", "Acting agent:"),
    ("tool_calls", "Tool calls this turn:"),
    ("response", "Assistant response to evaluate:"),
    ("user_prompt", "User prompt to evaluate:"),
    ("session_context", "Session context:"),
    ("analysis_findings", "Analysis findings:"),
    ("accumulated_ask", "Accumulated ask"),
    ("trajectory_digest", "Trajectory digest"),
    ("session_action_list", "Session action list"),
    ("prior_fires", "Prior fires this session"),
    ("thread_criteria", "Operator-approved acceptance criteria"),
    ("intake_criteria", "Acceptance criteria authored at intake"),
    ("checklist_instruction", "Populate `checklist`"),
    ("checklist_instruction", "Using the accumulated ask above"),
    ("earlier_notes", EARLIER_NOTES_HEADING),
    ("turn_opening", TURN_OPENING_HEADING),
    ("earlier_turns", EARLIER_TURNS_HEADING),
    ("recent_reactions", RECENT_REACTIONS_HEADING),
];

const NOT_AS_OF_SECTIONS: &[&str] = &["session_context", "thread_criteria"];

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RecordedCall {
    pub(crate) call_id: i64,
    pub(crate) turn_id: Option<i64>,
    pub(crate) trigger_request_id: Option<i64>,
    pub(crate) request_body: serde_json::Value,
    #[serde(default)]
    pub(crate) called_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub(crate) latency_ms: Option<i64>,
}

impl RecordedCall {
    fn send_moment(&self) -> Option<DateTime<Utc>> {
        let called_at = self.called_at?;
        Some(match self.latency_ms {
            Some(ms) => called_at - Duration::milliseconds(ms),
            None => called_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SectionDifference {
    pub(crate) section: String,
    pub(crate) first_differing_offset: usize,
    pub(crate) allowed_cause: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum ParityStatus {
    Equal,
    Differs { differences: Vec<SectionDifference> },
    Unmapped { reason: &'static str },
    LoadFailed { error: String },
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ParityRow {
    pub(crate) call_id: i64,
    pub(crate) as_of: &'static str,
    pub(crate) called_at: Option<DateTime<Utc>>,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) window_end: Option<DateTime<Utc>>,
    pub(crate) result: ParityStatus,
}

const AS_OF_TRIGGER_COMPLETED: &str = "trigger_completed_at";
const UNMAPPED_NO_TRIGGER: &str = "no trigger request maps to the recorded call";
const UNMAPPED_NO_TURN: &str = "no operator turn before the trigger";

fn section_key(chunk: &str) -> Option<&'static str> {
    SECTION_PREFIXES
        .iter()
        .find(|(_, prefix)| chunk.starts_with(prefix))
        .map(|(key, _)| *key)
}

pub(super) fn split_sections(message: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    for chunk in message.split("\n\n") {
        let headed = chunk.trim_start_matches('\n');
        match (section_key(headed), sections.last_mut()) {
            (Some(key), _) => sections.push((key.to_owned(), headed.to_owned())),
            (None, Some((_, body))) => {
                body.push_str("\n\n");
                body.push_str(chunk);
            }
            (None, None) => sections.push(("preamble".to_owned(), chunk.to_owned())),
        }
    }
    sections
}

fn first_differing_offset(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

pub(super) fn diff_sections(recorded: &str, rebuilt: &str) -> Vec<SectionDifference> {
    let left = split_sections(recorded);
    let right = split_sections(rebuilt);
    let keys: Vec<&String> = left.iter().chain(right.iter()).map(|(k, _)| k).collect();
    let mut seen: Vec<&String> = Vec::new();
    let mut out = Vec::new();
    for key in keys {
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let pick = |sections: &[(String, String)]| {
            sections
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, body)| body.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        let (l, r) = (pick(&left), pick(&right));
        if l != r {
            out.push(SectionDifference {
                section: key.clone(),
                first_differing_offset: first_differing_offset(&l, &r),
                allowed_cause: NOT_AS_OF_SECTIONS
                    .contains(&key.as_str())
                    .then_some("not rebuildable as of the call: no history exists"),
            });
        }
    }
    out
}

fn recorded_user_message(body: &serde_json::Value) -> Option<String> {
    body.get("messages")?
        .as_array()?
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(str::to_owned)
}

fn unmapped_reason(call: &RecordedCall) -> Option<&'static str> {
    match (call.turn_id, call.trigger_request_id) {
        (Some(_), Some(_)) => None,
        (None, Some(_)) => Some(UNMAPPED_NO_TURN),
        (_, None) => Some(UNMAPPED_NO_TRIGGER),
    }
}

async fn dump_pair(dir: &Path, call_id: i64, recorded: &str, rebuilt: &str) {
    for (suffix, text) in [("recorded", recorded), ("rebuilt", rebuilt)] {
        let path = dir.join(format!("{call_id}.{suffix}.txt"));
        if let Err(e) = tokio::fs::write(&path, text).await {
            tracing::warn!(path = %path.display(), err = %e, "parity dump: could not write");
        }
    }
}

async fn parity_one(
    ph: &PersistHandle,
    replay: &ReplaySteerer,
    call: &RecordedCall,
    dump_dir: Option<&Path>,
) -> ParityRow {
    let row = |result| ParityRow {
        call_id: call.call_id,
        as_of: AS_OF_TRIGGER_COMPLETED,
        called_at: call.called_at,
        latency_ms: call.latency_ms,
        window_end: call.send_moment(),
        result,
    };
    if let Some(reason) = unmapped_reason(call) {
        return row(ParityStatus::Unmapped { reason });
    }
    let (Some(turn_id), Some(trigger_request_id)) = (call.turn_id, call.trigger_request_id) else {
        return row(ParityStatus::Unmapped {
            reason: UNMAPPED_NO_TRIGGER,
        });
    };
    let Some(recorded) = recorded_user_message(&call.request_body) else {
        return row(ParityStatus::LoadFailed {
            error: "recorded request_body carries no user message".to_owned(),
        });
    };
    let item = ReplayItem {
        turn_id,
        trigger_request_id,
    };
    let assembled = assemble(ph, &item, replay, VerifierInputBudget::Uncapped).await;
    row(match assembled {
        Ok(inputs) => {
            if let Some(dir) = dump_dir {
                dump_pair(dir, call.call_id, &recorded, &inputs.user_message).await;
            }
            let differences = diff_sections(&recorded, &inputs.user_message);
            if differences.is_empty() {
                ParityStatus::Equal
            } else {
                ParityStatus::Differs { differences }
            }
        }
        Err(e) => ParityStatus::LoadFailed {
            error: e.to_string(),
        },
    })
}

pub(crate) async fn parity_recorded(
    ph: &PersistHandle,
    replay: &ReplaySteerer,
    calls: &[RecordedCall],
    dump_dir: Option<&Path>,
) -> Vec<ParityRow> {
    let mut rows = Vec::with_capacity(calls.len());
    for call in calls {
        rows.push(parity_one(ph, replay, call, dump_dir).await);
    }
    rows
}
