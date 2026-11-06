use serde_json::json;
use uuid::Uuid;

pub(super) fn brief(fleet: &str, session: Uuid, task: Option<&str>) -> String {
    let ask = json!({
        "from": session,
        "to": "<recipient>",
        "question": "<question>",
    });
    let answer = json!({
        "from": session,
        "question_id": "<question id>",
        "answer": "<answer>",
    });
    let note = json!({
        "session_id": session,
        "title": "<finding>",
    });
    let mut text = format!(
        "Examples only; replace <...> with real values. Do not call on startup:\nask {ask}\nanswer {answer}\nwrite_session_note {note}"
    );
    if let Some(id) = task {
        let report = report(fleet, session, id);
        text.push_str(&format!(
            "\nAfter fleet_commit, with real evidence:\nfleet_report {report}"
        ));
    }
    text
}

fn report(fleet: &str, session: Uuid, task_id: &str) -> serde_json::Value {
    json!({
        "fleet": fleet,
        "session_id": session,
        "task": task_id,
        "slice": "<finished slice>",
        "evidence": ["<evidence path>"],
    })
}

#[cfg(test)]
#[path = "tool_examples_tests.rs"]
mod tests;
