use rmcp::model::ErrorData;
use serde_json::{Map, Value};

use crate::profiles::{Profile, RETROSPECTIVE_TOOLS};

pub(crate) fn authorize(
    profile: Profile,
    name: &str,
    arguments: Option<Map<String, Value>>,
    session: Option<&str>,
) -> Result<Option<Map<String, Value>>, ErrorData> {
    if profile != Profile::Retrospective {
        return Ok(arguments);
    }
    if !RETROSPECTIVE_TOOLS.contains(&name) {
        return Err(denied(format!("tool {name} is unavailable")));
    }
    if !matches!(name, "write_session_note" | "end_workspace_registration") {
        return Ok(arguments);
    }
    let session = session
        .filter(|session| super::is_session_uuid(session))
        .ok_or_else(|| denied("writes require a valid trusted process session"))?;
    let mut arguments = arguments.unwrap_or_default();
    if arguments.get("session_id").is_some_and(|value| {
        !value
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(session))
    }) {
        return Err(denied("writes may target only the trusted process session"));
    }
    arguments.insert("session_id".to_string(), Value::String(session.to_string()));
    if name == "write_session_note" {
        findings_only(&mut arguments)?;
    }
    Ok(Some(arguments))
}

fn findings_only(arguments: &mut Map<String, Value>) -> Result<(), ErrorData> {
    if arguments
        .get("note_uuid")
        .is_some_and(|value| !value.is_null())
    {
        return Err(denied(
            "findings are new notes; existing notes cannot be updated",
        ));
    }
    if arguments.get("kind").is_some_and(|kind| kind != "note") {
        return Err(denied("findings must have kind note"));
    }
    if arguments
        .get("data")
        .is_some_and(|data| data.get("carry").is_some() || data.get("ruling").is_some())
    {
        return Err(denied(
            "findings cannot carry lessons or executable rulings",
        ));
    }
    if arguments
        .get("tags")
        .and_then(Value::as_array)
        .is_some_and(|tags| tags.iter().filter_map(Value::as_str).any(restricted_tag))
    {
        return Err(denied(
            "findings cannot carry lesson or acceptance-obligation tags",
        ));
    }
    arguments.insert("kind".to_string(), Value::String("note".to_string()));
    Ok(())
}

fn restricted_tag(tag: &str) -> bool {
    tag.starts_with("carry:") || tag == "acceptance-obligation"
}

fn denied(reason: impl std::fmt::Display) -> ErrorData {
    ErrorData::invalid_params(format!("retrospective profile: {reason}"), None)
}

#[cfg(test)]
#[path = "retrospective_tests.rs"]
mod tests;
