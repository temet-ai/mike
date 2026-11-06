use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::events::{EventKind, FleetEvent};
use super::status::{AcceptanceObligation, NoteFacts, Verdict};

pub(super) struct Evidence<'a> {
    pub(super) verdicts: Vec<Verdict>,
    pub(super) obligations: Vec<AcceptanceObligation>,
    pub(super) waiting: Vec<super::report::Waiting>,
    decisions: Vec<&'a NoteFacts>,
    judged: Vec<Judged>,
}

#[derive(Debug, Clone)]
struct Judged {
    at: DateTime<Utc>,
    task: String,
    criterion: String,
    passed: bool,
    needed: String,
}

impl<'a> Evidence<'a> {
    pub(super) fn new(fleet: &str, def: &FleetDefinition, notes: &'a [NoteFacts]) -> Self {
        let obligations = super::acceptance_register::register(fleet, def, notes);
        let mut decisions: Vec<&NoteFacts> = notes
            .iter()
            .filter(|note| note.kind == "decision" && authoritative(def, note))
            .collect();
        decisions.sort_by_key(|note| (note.updated_at, note.created_at));
        Self {
            verdicts: decisions.iter().filter_map(|note| verdict(note)).collect(),
            obligations,
            waiting: super::report::waiting(def, notes),
            decisions,
            judged: Vec::new(),
        }
    }

    pub(super) fn with_judge_verdicts(mut self, events: &[FleetEvent]) -> Self {
        let mut judged: Vec<Judged> = events.iter().filter_map(judged).collect();
        judged.sort_by_key(|entry| entry.at);
        self.verdicts.extend(judged.iter().map(judge_verdict));
        self.verdicts.sort_by_key(|verdict| verdict.at);
        self.judged = judged;
        self
    }

    fn judge_said(&self, task: &str, criterion: &str) -> Option<&Judged> {
        self.judged
            .iter()
            .rev()
            .find(|entry| entry.task == task && entry.criterion == criterion)
    }

    pub(super) fn report_waiting(&self, task: &str) -> bool {
        self.waiting.iter().any(|w| w.task == task)
    }

    pub(super) fn passed_criteria(&self, task: &str, criteria: &[String]) -> Vec<String> {
        criteria
            .iter()
            .filter(|criterion| self.criterion_passed(task, criterion))
            .cloned()
            .collect()
    }

    fn criterion_passed(&self, task: &str, criterion: &str) -> bool {
        let Some(at) = self
            .note_pass_at(task, criterion)
            .or_else(|| self.judge_pass_at(task, criterion))
        else {
            return false;
        };
        self.obligations
            .iter()
            .filter(|obligation| obligation.required && obligation.covers(task, criterion))
            .all(|obligation| obligation.accepted_for(task) && at >= obligation.at)
    }

    fn note_pass_at(&self, task: &str, criterion: &str) -> Option<DateTime<Utc>> {
        let note = self.decisions.iter().rev().find(|note| {
            tasks_named(&note.data).iter().any(|id| id == task)
                && criterion_named(&note.data, criterion)
                && final_verdict(&note.data, self.explicit_for(task, criterion))
        })?;
        verdict_is(&note.data, "pass").then_some(note.updated_at)
    }

    fn judge_pass_at(&self, task: &str, criterion: &str) -> Option<DateTime<Utc>> {
        let judged = self.judge_said(task, criterion)?;
        judged.passed.then_some(judged.at)
    }

    fn explicit_for(&self, task: &str, criterion: &str) -> bool {
        self.obligations
            .iter()
            .any(|entry| entry.covers(task, criterion))
            || self.decisions.iter().any(|note| {
                note.data.get("readiness").is_some()
                    && tasks_named(&note.data).iter().any(|id| id == task)
            })
    }

    pub(super) fn ready(&self, task: &str, accepted: bool) -> bool {
        let latest = self.decisions.iter().rev().find(|note| {
            tasks_named(&note.data).iter().any(|id| id == task)
                && note.data.get("readiness").is_some()
        });
        latest.map_or(accepted, |note| {
            note.data.get("readiness").and_then(Value::as_str) == Some("ready")
        })
    }

    pub(super) fn failed(&self, task: &str, criteria: &[String]) -> bool {
        criteria.iter().any(|criterion| {
            self.note_failed(task, criterion)
                || self
                    .judge_said(task, criterion)
                    .is_some_and(|judged| !judged.passed)
                || self.obligations.iter().any(|obligation| {
                    obligation.required
                        && obligation.covers(task, criterion)
                        && obligation.outcome == "failed"
                })
        })
    }

    fn note_failed(&self, task: &str, criterion: &str) -> bool {
        self.decisions
            .iter()
            .rev()
            .find(|note| {
                tasks_named(&note.data).iter().any(|id| id == task)
                    && criterion_named(&note.data, criterion)
                    && progress_verdict(&note.data)
            })
            .is_some_and(|note| verdict_is(&note.data, "fail"))
    }
}

fn judged(event: &FleetEvent) -> Option<Judged> {
    if event.kind != EventKind::VerdictRecorded || event.session_uuid.is_some() {
        return None;
    }
    Some(Judged {
        at: event.occurred_at,
        task: event.task.clone()?,
        criterion: event.payload.get("criterion")?.as_str()?.to_string(),
        passed: event.payload.get("passed")?.as_bool()?,
        needed: event
            .payload
            .get("needed")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn judge_verdict(judged: &Judged) -> Verdict {
    let needed = judged.needed.trim();
    let needed = if needed.is_empty() {
        String::new()
    } else {
        format!("; needed: {needed}")
    };
    Verdict {
        at: judged.at,
        verdict: if judged.passed { "pass" } else { "fail" }.to_string(),
        title: format!(
            "{} of task {} by the judge{needed}",
            judged.criterion, judged.task
        ),
        tasks: vec![judged.task.clone()],
        criteria: vec![judged.criterion.clone()],
    }
}

pub(super) fn authoritative(def: &FleetDefinition, note: &NoteFacts) -> bool {
    let authored_task = def.tasks.values().any(|task| {
        task.session
            .as_deref()
            .and_then(|id| id.parse::<Uuid>().ok())
            == Some(note.session_uuid)
    });
    if authored_task {
        return false;
    }
    let configured = def
        .acceptance
        .session
        .as_deref()
        .and_then(|id| id.parse::<Uuid>().ok());
    configured == Some(note.session_uuid)
        || (note.role.as_deref() == Some("acceptance")
            && note.seat.as_deref().is_some_and(acceptance_seat))
}

fn acceptance_seat(seat: &str) -> bool {
    seat == "acceptance"
        || seat.strip_prefix("acceptance-").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn final_verdict(data: &Value, explicit: bool) -> bool {
    verdict_is(data, "fail")
        || (verdict_is(data, "pass")
            && ((!explicit && data.get("slice").is_none() && data.get("scope").is_none())
                || data.get("scope").and_then(Value::as_str) == Some("final")))
}

fn progress_verdict(data: &Value) -> bool {
    final_verdict(data, false)
        || (verdict_is(data, "pass")
            && (data.get("scope").and_then(Value::as_str) == Some("slice")
                || (data.get("scope").is_none()
                    && data.get("slice").is_some_and(|slice| {
                        slice.is_number()
                            || slice.as_str().is_some_and(|name| !name.trim().is_empty())
                    }))))
}

fn criterion_named(data: &Value, criterion: &str) -> bool {
    let Some(criteria) = data.get("criteria").and_then(Value::as_array) else {
        return verdict_is(data, "fail");
    };
    if criteria.is_empty() {
        return verdict_is(data, "fail");
    }
    criteria
        .iter()
        .all(|value| value.as_str().is_some_and(|id| !id.trim().is_empty()))
        && criteria.iter().any(|id| id.as_str() == Some(criterion))
}

fn verdict_is(data: &Value, word: &str) -> bool {
    data.get("verdict")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case(word))
}

fn verdict(note: &NoteFacts) -> Option<Verdict> {
    let word = note.data.get("verdict")?.as_str()?.to_lowercase();
    if word != "pass" && word != "fail" {
        return None;
    }
    let title = note
        .title
        .strip_prefix("PASS ")
        .or_else(|| note.title.strip_prefix("FAIL "))
        .unwrap_or(&note.title)
        .to_string();
    Some(Verdict {
        at: note.updated_at,
        verdict: word,
        title,
        tasks: tasks_named(&note.data),
        criteria: strings(&note.data, "criteria"),
    })
}

pub(super) fn strings(data: &Value, key: &str) -> Vec<String> {
    let mut values: Vec<String> = data
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    values.sort();
    values.dedup();
    values
}

pub(super) fn tasks_named(data: &Value) -> Vec<String> {
    let one = ["task", "package"]
        .into_iter()
        .filter_map(|key| data.get(key).and_then(Value::as_str))
        .map(str::to_string);
    let many = ["tasks", "packages"]
        .into_iter()
        .flat_map(|key| strings(data, key));
    let mut names: Vec<String> = one.chain(many).collect();
    names.sort();
    names.dedup();
    names
}

pub(super) fn note_time(note: &NoteFacts) -> (DateTime<Utc>, DateTime<Utc>) {
    (note.created_at, note.updated_at)
}
