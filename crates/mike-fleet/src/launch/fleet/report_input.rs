use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::report::{ReportRequest, task_session};
use super::seated::Seated;

const TEXT_KEYS: [&str; 7] = [
    "text",
    "summary",
    "criterion",
    "description",
    "item",
    "reason",
    "what",
];

fn entry_text(value: Value) -> String {
    match value {
        Value::String(text) => text,
        Value::Object(fields) => {
            let found = TEXT_KEYS
                .iter()
                .find_map(|key| fields.get(*key).and_then(Value::as_str));
            match (found, fields.get("criterion").and_then(Value::as_str)) {
                (Some(text), Some(criterion)) if text != criterion => {
                    format!("{criterion}: {text}")
                }
                (Some(text), _) => text.to_string(),
                (None, _) => Value::Object(fields).to_string(),
            }
        }
        other => other.to_string(),
    }
}

pub(super) fn entries_or_none<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Input {
        List(Vec<Value>),
        One(Value),
    }
    Ok(match Option::<Input>::deserialize(deserializer)? {
        None | Some(Input::One(Value::Null)) => None,
        Some(Input::List(values)) => Some(values.into_iter().map(entry_text).collect()),
        Some(Input::One(value)) => Some(vec![entry_text(value)]),
    })
}

fn tasks_of(def: &FleetDefinition, seated: &Seated, session: Uuid) -> Vec<String> {
    let mut held: Vec<String> = def
        .tasks
        .iter()
        .filter(|(_, task)| task_session(task) == Some(session))
        .map(|(id, _)| id.clone())
        .collect();
    if let Some(task) = seated.task_of(session)
        && !held.iter().any(|id| id == task)
    {
        held.push(task.to_string());
    }
    held.sort();
    held
}

pub fn fill_seat_defaults(
    req: &mut ReportRequest,
    def: &FleetDefinition,
    seated: &Seated,
) -> Result<(), String> {
    if req.task.trim().is_empty() {
        let session = req
            .session
            .ok_or("pass your own session id as `session_id`")?;
        let held = tasks_of(def, seated, session);
        match held.as_slice() {
            [only] => req.task.clone_from(only),
            [] => {
                let all: Vec<&str> = def.tasks.keys().map(String::as_str).collect();
                return Err(format!(
                    "name the task you report (`task`): session {session} holds none; the fleet's tasks are {}",
                    all.join(", ")
                ));
            }
            many => {
                return Err(format!(
                    "name the task you report (`task`): session {session} holds {}",
                    many.join(", ")
                ));
            }
        }
    }
    if req.slice.trim().is_empty() {
        req.slice = format!("task {}", req.task);
    }
    Ok(())
}
