use rmcp::model::{ErrorData, RequestMetaObject};
use serde_json::Value;

use crate::client::InspectClient;

pub(crate) fn process_thread(
    claude_session: Option<&str>,
    codex_thread: Option<String>,
) -> Option<String> {
    if claude_session.is_some_and(super::is_session_uuid) {
        None
    } else {
        codex_thread
    }
}

pub(crate) fn request_thread(meta: Option<&RequestMetaObject>) -> Result<Option<&str>, ErrorData> {
    let Some(meta) = meta else {
        return Ok(None);
    };
    let direct = meta.get("threadId");
    let nested = meta
        .get("x-codex-turn-metadata")
        .map(|value| {
            value.get("thread_id").ok_or_else(|| {
                ErrorData::invalid_params(
                    "Codex request metadata x-codex-turn-metadata.thread_id is missing",
                    None,
                )
            })
        })
        .transpose()?;
    let Some(value) = direct.or(nested) else {
        return Ok(None);
    };
    let thread = value
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ErrorData::invalid_params(
                "Codex request metadata must contain a non-empty thread id",
                None,
            )
        })?;
    if let Some(nested) = nested
        && direct.is_some()
        && nested.as_str() != Some(thread)
    {
        return Err(ErrorData::invalid_params(
            "Codex request metadata contains conflicting thread ids",
            None,
        ));
    }
    Ok(Some(thread))
}

pub(crate) async fn resolve(
    client: &InspectClient,
    native_thread: Option<&str>,
    process_session: Option<&str>,
) -> Result<Option<String>, ErrorData> {
    let Some(thread) = native_thread else {
        return Ok(process_session
            .filter(|id| super::is_session_uuid(id))
            .map(str::to_string));
    };
    let response = client
        .get_json("/api/codex/session", &[("thread_id", thread.to_string())])
        .await
        .map_err(|error| ErrorData::invalid_params(
            format!("Cannot resolve Codex thread {thread:?} to a Mike session: {error}. Check that Mike is observing this Codex session."),
            None,
        ))?;
    response
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|id| super::is_session_uuid(id))
        .map(|id| Some(id.to_string()))
        .ok_or_else(|| {
            ErrorData::invalid_params(
                format!("Mike returned no valid session_id for Codex thread {thread:?}"),
                None,
            )
        })
}

pub(crate) fn requires_identity(name: &str) -> bool {
    matches!(
        name,
        "send_session_message"
            | "ask"
            | "answer"
            | "decline"
            | "write_session_note"
            | "work_thread_checkpoint"
            | "work_thread_add_entry"
            | "define_fleet"
            | "ack_session_message"
            | "workspace_heartbeat"
    )
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
