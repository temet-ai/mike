use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::acceptance::{authoritative, note_time, strings, tasks_named};
use super::definition::FleetDefinition;
use super::status::{AcceptanceObligation, NoteFacts};

const OUTCOMES: [&str; 6] = [
    "pending",
    "blocked",
    "accepted",
    "failed",
    "not_exercised",
    "invalidated",
];

struct Registered {
    current: AcceptanceObligation,
    scope: Option<Scope>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, PartialEq, Eq)]
struct Scope {
    tasks: Vec<String>,
    criteria: Vec<String>,
}

impl Scope {
    fn of(obligation: &AcceptanceObligation, def: &FleetDefinition) -> Option<Self> {
        scope_valid(obligation, def).then(|| Self {
            tasks: obligation.tasks.clone(),
            criteria: obligation.criteria.clone(),
        })
    }

    fn matches(&self, obligation: &AcceptanceObligation) -> bool {
        self.tasks == obligation.tasks && self.criteria == obligation.criteria
    }
}

pub(super) fn register(
    fleet: &str,
    def: &FleetDefinition,
    notes: &[NoteFacts],
) -> Vec<AcceptanceObligation> {
    let fleet_tag = format!("fleet:{fleet}");
    let mut relevant: Vec<&NoteFacts> = notes
        .iter()
        .filter(|note| {
            note.kind == "task" && note.tags.iter().any(|tag| tag == "acceptance-obligation")
        })
        .filter(|note| {
            !note.tags.iter().any(|tag| tag.starts_with("fleet:")) || note.tags.contains(&fleet_tag)
        })
        .collect();
    relevant.sort_by_key(|note| note_time(note));
    let mut current = BTreeMap::<String, Registered>::new();
    for note in relevant {
        let mut obligation = obligation(&fleet_tag, def, note);
        let previous = current.get(&obligation.obligation_id);
        successor(&mut obligation, previous, note);
        let scope = previous
            .and_then(|entry| entry.scope.clone())
            .or_else(|| Scope::of(&obligation, def));
        let updated_at = previous.map_or(obligation.updated_at, |entry| {
            entry.updated_at.max(obligation.updated_at)
        });
        current.insert(
            obligation.obligation_id.clone(),
            Registered {
                current: obligation,
                scope,
                updated_at,
            },
        );
    }
    current.into_values().map(scoped_current).collect()
}

fn scoped_current(entry: Registered) -> AcceptanceObligation {
    let mut current = entry.current;
    match entry.scope {
        Some(scope) => {
            current.tasks = scope.tasks;
            current.criteria = scope.criteria;
        }
        None => {
            current.tasks.clear();
            current.criteria.clear();
        }
    }
    current
}

fn obligation(fleet_tag: &str, def: &FleetDefinition, note: &NoteFacts) -> AcceptanceObligation {
    let id = note
        .data
        .get("obligation_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty());
    let mut obligation = AcceptanceObligation {
        note_id: note.note_uuid,
        session_uuid: note.session_uuid,
        obligation_id: id.map_or_else(|| format!("invalid:{}", note.note_uuid), str::to_string),
        title: note.title.clone(),
        body: note.body.clone(),
        data: note.data.clone(),
        tasks: tasks_named(&note.data),
        criteria: strings(&note.data, "criteria"),
        outcome: note
            .data
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("pending")
            .to_string(),
        required: note
            .data
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        issue: None,
        at: note.created_at,
        updated_at: note.updated_at,
        ruling_proof: Vec::new(),
    };
    if id.is_none() {
        issue(&mut obligation, "missing obligation_id");
    }
    if !note.tags.iter().any(|tag| tag == fleet_tag) {
        issue(&mut obligation, "missing fleet tag");
    }
    if !authoritative(def, note) {
        issue(
            &mut obligation,
            "the author is not the fleet's acceptance owner",
        );
    }
    validate(&mut obligation, def);
    obligation
}

fn validate(obligation: &mut AcceptanceObligation, def: &FleetDefinition) {
    if let Err(problem) = super::ruling::parse(&obligation.data) {
        issue(obligation, &problem);
    }
    if !scope_valid(obligation, def) {
        issue(
            obligation,
            "missing, unknown or inconsistent task and criterion scope",
        );
    }
    if !OUTCOMES.contains(&obligation.outcome.as_str())
        || obligation
            .data
            .get("outcome")
            .and_then(Value::as_str)
            .is_none()
    {
        issue(obligation, "missing or unknown outcome");
    }
    if obligation
        .data
        .get("required")
        .is_some_and(|value| !value.is_boolean())
    {
        issue(obligation, "required must be a boolean");
    }
    if obligation.outcome == "accepted" && !observed(&obligation.data) {
        issue(
            obligation,
            "accepted obligation requires observation or observation_notes",
        );
    }
    if malformed_scope(&obligation.data) {
        issue(
            obligation,
            "task and criterion scope must contain nonempty string ids",
        );
    }
}

fn scope_valid(obligation: &AcceptanceObligation, def: &FleetDefinition) -> bool {
    if malformed_scope(&obligation.data)
        || (obligation.tasks.is_empty() && obligation.criteria.is_empty())
        || obligation
            .criteria
            .iter()
            .any(|id| !def.criteria.contains_key(id))
    {
        return false;
    }
    obligation.tasks.iter().all(|id| {
        def.tasks.get(id).is_some_and(|task| {
            obligation.criteria.is_empty()
                || obligation
                    .criteria
                    .iter()
                    .any(|criterion| task.criteria.contains(criterion))
        })
    })
}

fn malformed_scope(data: &Value) -> bool {
    ["task", "package"].iter().any(|key| {
        data.get(key)
            .is_some_and(|value| value.as_str().is_none_or(|id| id.trim().is_empty()))
    }) || ["tasks", "packages", "criteria"].iter().any(|key| {
        data.get(key).is_some_and(|value| {
            value.as_array().is_none_or(|values| {
                values
                    .iter()
                    .any(|value| value.as_str().is_none_or(|id| id.trim().is_empty()))
            })
        })
    })
}

fn observed(data: &Value) -> bool {
    data.get("observation")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
        || data
            .get("observation_notes")
            .and_then(Value::as_array)
            .is_some_and(|notes| {
                !notes.is_empty()
                    && notes
                        .iter()
                        .all(|note| note.as_str().is_some_and(|id| id.parse::<Uuid>().is_ok()))
            })
}

fn successor(
    obligation: &mut AcceptanceObligation,
    previous: Option<&Registered>,
    note: &NoteFacts,
) {
    let supersedes = note
        .data
        .get("supersedes")
        .and_then(Value::as_str)
        .and_then(|id| id.parse::<Uuid>().ok());
    match previous {
        Some(previous) => {
            if previous.current.data.get("ruling").is_some() {
                let before = super::ruling::parse(&previous.current.data).ok().flatten();
                let after = super::ruling::parse(&obligation.data).ok().flatten();
                if before
                    .zip(after)
                    .is_none_or(|(before, after)| after.version <= before.version)
                {
                    issue(
                        obligation,
                        "successor must preserve an executable ruling with a higher version",
                    );
                }
            }
            if supersedes != Some(previous.current.note_id) {
                issue(obligation, "successor must supersede the current note UUID");
            }
            if previous
                .scope
                .as_ref()
                .is_some_and(|scope| !scope.matches(obligation))
            {
                issue(
                    obligation,
                    "successor must preserve the obligation's task and criterion scope",
                );
            }
            if previous.updated_at > obligation.updated_at {
                issue(
                    obligation,
                    "successor predates an update to the superseded note",
                );
            }
            obligation.required |= previous.current.required;
        }
        None if note.data.get("supersedes").is_some() => {
            issue(
                obligation,
                "superseded note is missing or names another obligation",
            );
        }
        None => {}
    }
}

fn issue(obligation: &mut AcceptanceObligation, message: &str) {
    match &mut obligation.issue {
        Some(existing) => {
            existing.push_str("; ");
            existing.push_str(message);
        }
        None => obligation.issue = Some(message.to_string()),
    }
    obligation.required = true;
}

pub(crate) fn validate_write(
    fleet: &str,
    def: &FleetDefinition,
    note: &NoteFacts,
) -> Result<(), String> {
    if note.kind != "task" {
        return Err("acceptance-obligation requires kind task".into());
    }
    let checked = obligation(&format!("fleet:{fleet}"), def, note);
    match checked.issue {
        Some(issue) => Err(issue),
        None => Ok(()),
    }
}

pub(crate) fn equivalent_scope(before: &Value, after: &Value) -> bool {
    !malformed_scope(before)
        && !malformed_scope(after)
        && tasks_named(before) == tasks_named(after)
        && strings(before, "criteria") == strings(after, "criteria")
}
