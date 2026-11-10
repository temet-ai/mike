use rmcp::model::ErrorData;
use serde_json::{Map, Value};

use crate::client::InspectClient;

pub(crate) mod retrospective;

const SESSION_TAGGED_TOOLS: [&str; 5] = [
    "work_thread_checkpoint",
    "work_thread_add_entry",
    "search_project_knowledge",
    "recall_similar_turns",
    "recall_project_context",
];

pub(crate) async fn supply_defaults(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
    session: Option<&str>,
) -> Result<Option<Map<String, Value>>, ErrorData> {
    let arguments = supply_required(client, name, arguments, session).await?;
    Ok(supply_writing_session(name, arguments, session))
}

fn supply_writing_session(
    name: &str,
    arguments: Option<Map<String, Value>>,
    session: Option<&str>,
) -> Option<Map<String, Value>> {
    let Some(mut arguments) = arguments else {
        return arguments;
    };
    let named = arguments.contains_key("session_id") || arguments.contains_key("session_uuid");
    if let Some(session) = session
        .filter(|value| SESSION_TAGGED_TOOLS.contains(&name) && !named && is_session_uuid(value))
    {
        arguments.insert("session_id".to_string(), Value::String(session.to_string()));
    }
    Some(arguments)
}

async fn supply_required(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
    session: Option<&str>,
) -> Result<Option<Map<String, Value>>, ErrorData> {
    let field = match name {
        "ask" | "answer" | "decline" => "from",
        "write_session_note" => "session_id",
        "work_thread_list"
        | "work_thread_get"
        | "work_thread_checkpoint"
        | "work_thread_add_entry" => "project",
        _ => return Ok(arguments),
    };
    let mut arguments = arguments.unwrap_or_default();
    if arguments.contains_key(field) {
        return Ok(Some(arguments));
    }
    let session = session
        .filter(|value| is_session_uuid(value))
        .ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "{name} requires `{field}` when trusted caller context is unavailable. \
                 Pass it explicitly, or launch mike-mcp with a valid CLAUDE_CODE_SESSION_ID \
                 (or MIKE_SESSION_ID), for example 00000000-0000-4000-8000-000000000001."
                ),
                None,
            )
        })?;
    let value = if field == "project" {
        caller_project(client, name, session).await?
    } else {
        session.to_string()
    };
    arguments.insert(field.to_string(), Value::String(value));
    Ok(Some(arguments))
}

fn is_session_uuid(value: &str) -> bool {
    value.len() == 36
        && value.char_indices().all(|(index, character)| match index {
            8 | 13 | 18 | 23 => character == '-',
            _ => character.is_ascii_hexdigit(),
        })
}

async fn caller_project(
    client: &InspectClient,
    name: &str,
    session: &str,
) -> Result<String, ErrorData> {
    let detail = client
        .get_session(session, None, Some(1), None)
        .await
        .map_err(crate::error::McpError::into_rmcp)?;
    if !detail
        .pointer("/summary/id")
        .and_then(Value::as_str)
        .is_some_and(|id| id.eq_ignore_ascii_case(session))
    {
        return Err(ErrorData::invalid_params(
            format!(
                "{name} could not verify the caller's session summary; pass `project` explicitly."
            ),
            None,
        ));
    }
    detail
        .pointer("/summary/project")
        .and_then(Value::as_str)
        .filter(|project| !project.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "{name} could not resolve the caller's project; pass `project` \
                     explicitly as a project UUID or unambiguous registered path."
                ),
                None,
            )
        })
}

#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod own_turn_tests;
#[cfg(test)]
mod tests;
