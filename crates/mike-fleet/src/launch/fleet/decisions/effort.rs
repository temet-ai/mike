use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::{Value, json};

use super::config::{Mode, Point};
use super::{FleetDecisions, text};
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, choice};
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::effort::{
    EffortRow, EffortShare, Labelled, SeatRequest, Unlabelled,
};
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub const QUESTION_SET: &str = "fleet_effort_v1";
pub const OBSERVE_BATCH: i64 = 300;
pub const LABEL_BATCH: i64 = 8;
const LABEL_EVERY: Duration = Duration::from_secs(15);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(120);
const STEP_TEXT_BYTES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Operator,
    Hook,
    Mail,
    Wakeup,
    ToolResult,
    Steering,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::Hook => "hook",
            Self::Mail => "mail",
            Self::Wakeup => "wakeup",
            Self::ToolResult => "tool_result",
            Self::Steering => "steering",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Work,
    Machinery,
    Waiting,
}

impl Label {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Machinery => "machinery",
            Self::Waiting => "waiting",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "work" => Some(Self::Work),
            "machinery" => Some(Self::Machinery),
            "waiting" => Some(Self::Waiting),
            _ => None,
        }
    }
}

const HOOK_OPENERS: [&str; 4] = [
    "A session-scoped Stop hook",
    "Stop hook feedback:",
    "/goal",
    "Your brief is saved at ",
];
const WAKEUP_OPENERS: [&str; 2] = ["Coordinator wake", "coordinator wake"];

fn silent_wake(text: &str) -> bool {
    text.starts_with("You have ") && text.contains("have been silent")
}
const MAIL_MARKERS: [&str; 4] = ["<channel ", "mike-mail:", "[mike ", "<mike-mail"];

pub fn request_trigger(request: &SeatRequest) -> Trigger {
    let text = request.text.trim_start();
    if request.turn_origin == "steered" {
        return Trigger::Steering;
    }
    if request.turn_type == "tool_loop" {
        return Trigger::ToolResult;
    }
    if request.turn_origin.ends_with("_sidecar") || request.turn_origin == "quota_probe" {
        return Trigger::Hook;
    }
    if HOOK_OPENERS.iter().any(|opener| text.starts_with(opener)) {
        return Trigger::Hook;
    }
    if silent_wake(text) || WAKEUP_OPENERS.iter().any(|opener| text.starts_with(opener)) {
        return Trigger::Wakeup;
    }
    if MAIL_MARKERS.iter().any(|marker| text.contains(marker)) {
        return Trigger::Mail;
    }
    match request.author_kind.as_str() {
        "operator" => Trigger::Operator,
        _ => Trigger::Hook,
    }
}

pub fn trigger_label(trigger: Trigger) -> Option<Label> {
    match trigger {
        Trigger::Hook | Trigger::Steering => Some(Label::Machinery),
        _ => None,
    }
}

pub fn effort_row(request: &SeatRequest) -> EffortRow {
    let trigger = request_trigger(request);
    let label = trigger_label(trigger);
    EffortRow {
        request_id: request.request_id,
        seat: request.seat.clone(),
        session_id: request.session_id,
        trigger: trigger.as_str().to_owned(),
        label: label.map(|l| l.as_str().to_owned()),
        decided_by: label.map(|_| "trigger".to_owned()),
    }
}

pub async fn observe(persist: &PersistHandle, run: &str) -> u64 {
    let Ok(requests) = persist.fleet().new_seat_requests(run, OBSERVE_BATCH).await else {
        return 0;
    };
    let rows: Vec<EffortRow> = requests.iter().map(effort_row).collect();
    persist
        .fleet()
        .insert_request_efforts(run, &rows)
        .await
        .unwrap_or(0)
}

fn step_key(request_id: i64) -> String {
    format!("step_{request_id}")
}

pub fn questions(steps: &[Unlabelled]) -> BTreeMap<String, Question> {
    steps
        .iter()
        .map(|step| {
            (
                step_key(step.request_id),
                choice(
                    &format!("What was the seat doing in step {}?", step.request_id),
                    &[
                        ("work", "Reading, searching, editing or testing the project's own code or files to do the task."),
                        ("machinery", "Using the fleet's protocol rather than the project: checks, commits, reports, mail, status, roster, build permits, hooks."),
                        ("waiting", "Waiting or polling for something else to finish, sleeping, or idle."),
                    ],
                ),
            )
        })
        .collect()
}

pub fn state(steps: &[Unlabelled]) -> Value {
    let rows: Vec<Value> = steps
        .iter()
        .map(|step| {
            json!({
                "step": step.request_id,
                "seat": step.seat,
                "started_by": step.trigger,
                "prompt": text::tail(&step.text, STEP_TEXT_BYTES),
                "tool_calls": text::head(&step.tools, STEP_TEXT_BYTES),
            })
        })
        .collect();
    json!({ "steps": rows })
}

pub fn compose(asked: &Asked, steps: &[Unlabelled]) -> Vec<(i64, Label, f64)> {
    if asked.failed() {
        return Vec::new();
    }
    steps
        .iter()
        .filter_map(|step| {
            let (key, p) = asked.choice(&step_key(step.request_id))?;
            Some((step.request_id, Label::from_key(key)?, p))
        })
        .collect()
}

static LAST_LABELLED: LazyLock<DashMap<String, Instant>> = LazyLock::new(DashMap::new);

fn due(run: &str, now: Instant) -> bool {
    LAST_LABELLED.get(run).is_none_or(|next| now >= *next)
}

fn paced(run: &str, wait: Duration) {
    LAST_LABELLED.insert(run.to_owned(), Instant::now() + wait);
}

pub async fn label_pass(
    decisions: Option<&FleetDecisions>,
    mode: Mode,
    persist: &PersistHandle,
    (run, fleet_id): (&str, i64),
) -> usize {
    let Some(decider) = decisions.and_then(|d| d.decider(Point::Effort, mode)) else {
        return 0;
    };
    if !due(run, Instant::now()) || !decider.applies() {
        return 0;
    }
    let Ok(steps) = persist.fleet().unlabelled_efforts(run, LABEL_BATCH).await else {
        return 0;
    };
    if steps.is_empty() {
        return 0;
    }
    let subject = DecisionSubject {
        fleet_run_id: Some(fleet_id),
        ..DecisionSubject::default()
    };
    let asked = decider
        .ask(
            QUESTION_SET,
            state(&steps),
            questions(&steps),
            subject,
            json!({}),
        )
        .await;
    let chosen = compose(&asked, &steps);
    let score = if chosen.is_empty() {
        1.0
    } else {
        chosen.iter().map(|(_, _, p)| p).sum::<f64>() / chosen.len() as f64
    };
    let applied = if chosen.is_empty() {
        "none"
    } else {
        "labelled"
    };
    decider
        .mark(&asked, asked.composed(json!({"score": score})), applied)
        .await;
    paced(
        run,
        if chosen.is_empty() {
            RETRY_AFTER_FAILURE
        } else {
            LABEL_EVERY
        },
    );
    let labelled: Vec<Labelled> = chosen
        .iter()
        .map(|(id, label, _)| Labelled {
            request_id: *id,
            label: label.as_str().to_owned(),
            decided_by: "jev".to_owned(),
            record_id: asked.record_id,
        })
        .collect();
    persist
        .fleet()
        .label_request_efforts(&labelled)
        .await
        .map_or(0, |n| n as usize)
}

pub fn shares_document(rows: &[EffortShare]) -> Value {
    let mut by_seat: BTreeMap<&str, BTreeMap<&str, (i64, i64)>> = BTreeMap::new();
    let mut total: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    for row in rows {
        let seat = by_seat.entry(row.seat.as_str()).or_default();
        let cell = seat.entry(row.label.as_str()).or_default();
        cell.0 += row.requests;
        cell.1 += row.tokens;
        let all = total.entry(row.label.as_str()).or_default();
        all.0 += row.requests;
        all.1 += row.tokens;
    }
    json!({
        "basis": "input plus output tokens per request, labelled work, machinery or waiting; `unlabelled` has no label yet",
        "all_seats": cell_document(&total),
        "by_seat": by_seat.iter().map(|(seat, cells)| ((*seat).to_owned(), cell_document(cells))).collect::<BTreeMap<_, _>>(),
    })
}

fn cell_document(cells: &BTreeMap<&str, (i64, i64)>) -> Value {
    let tokens: i64 = cells.values().map(|(_, t)| t).sum();
    let mut shares = serde_json::Map::new();
    for (label, (requests, label_tokens)) in cells {
        let share = (tokens > 0)
            .then(|| (100.0 * *label_tokens as f64 / tokens as f64 * 10.0).round() / 10.0);
        shares.insert(
            (*label).to_owned(),
            json!({"requests": requests, "tokens": label_tokens, "share_percent": share}),
        );
    }
    json!({"tokens": tokens, "labels": shares})
}

pub async fn tick(
    state: &crate::base_state::BaseState,
    persist: &PersistHandle,
    (row, def): (
        &crate::store::persistence::fleet::FleetRow,
        &super::super::definition::FleetDefinition,
    ),
    run: &str,
) {
    observe(persist, run).await;
    let mode = def.decision_mode(Point::Effort, None);
    let decisions = FleetDecisions::from_state(state);
    label_pass(decisions.as_ref(), mode, persist, (run, row.id)).await;
}

pub async fn document(persist: &PersistHandle, run: &str) -> Value {
    match persist.fleet().effort_shares(run).await {
        Ok(rows) => shares_document(&rows),
        Err(error) => json!({"error": error.to_string()}),
    }
}

#[cfg(test)]
#[path = "effort_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "effort_db_tests.rs"]
mod db_tests;
