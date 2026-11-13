use std::collections::HashSet;

use super::PersistError;
use super::request_write_tool_io::InsertedToolResult;
use crate::extract::agent_result;
use crate::extract::content::extract_text_from_content;
use crate::store::models::{InterceptedRequest, ToolResult};

pub async fn write_worker_producer_ids(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
) -> Result<(), PersistError> {
    let mut wire_tool_use_ids: Vec<String> = Vec::new();
    let mut producer_ids: Vec<String> = Vec::new();
    for result in &req.tool_results_in_body {
        let Some(producer_id) = agent_result::worker_producer_id(&result.content) else {
            continue;
        };
        wire_tool_use_ids.push(result.tool_use_id.clone());
        producer_ids.push(producer_id);
    }
    if wire_tool_use_ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "UPDATE agents.dispatches d
            SET worker_producer_id = u.producer_id
           FROM UNNEST($1::text[], $2::text[]) AS u(wire_tool_use_id, producer_id)
           JOIN tools.uses tu ON tu.wire_tool_use_id = u.wire_tool_use_id
          WHERE d.tool_use_id = tu.id
            AND tu.tool_name = 'Agent'
            AND d.worker_producer_id IS DISTINCT FROM u.producer_id",
        &wire_tool_use_ids,
        &producer_ids,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

const DISPATCH_TOOL_NAMES: [&str; 4] =
    ["Agent", "Task", "spawn_session", "mcp__mike__spawn_session"];

pub fn dispatch_result_wire_ids(
    results: &[ToolResult],
    inserted: &[InsertedToolResult],
) -> Vec<String> {
    let newly_delivered: HashSet<&str> = inserted
        .iter()
        .filter(|row| DISPATCH_TOOL_NAMES.contains(&row.tool_name.as_str()))
        .map(|row| row.wire_tool_use_id.as_str())
        .collect();
    results
        .iter()
        .filter(|result| newly_delivered.contains(result.tool_use_id.as_str()))
        .filter(|result| is_dispatch_result(&result.content))
        .map(|result| result.tool_use_id.clone())
        .collect()
}

fn is_dispatch_result(content: &serde_json::Value) -> bool {
    agent_result::worker_producer_id(content).is_some() || carries_launched_session(content)
}

fn carries_launched_session(content: &serde_json::Value) -> bool {
    let blocks = match content {
        serde_json::Value::Array(blocks) => blocks.as_slice(),
        other => std::slice::from_ref(other),
    };
    blocks.iter().any(|block| {
        launched_session(block).is_some()
            || serde_json::from_str::<serde_json::Value>(&extract_text_from_content(
                &serde_json::Value::Array(vec![block.clone()]),
            ))
            .ok()
            .is_some_and(|payload| launched_session(&payload).is_some())
            || block
                .as_str()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                .is_some_and(|payload| launched_session(&payload).is_some())
    })
}

fn launched_session(payload: &serde_json::Value) -> Option<&str> {
    payload.pointer("/launch/session")?.as_str()
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    fn result(id: &str, content: serde_json::Value) -> ToolResult {
        ToolResult {
            tool_use_id: id.to_string(),
            is_error: false,
            content_sha256: String::new(),
            content_bytes: 0,
            content,
        }
    }

    fn agent_content() -> serde_json::Value {
        json!([{"type": "text", "text": "Async agent launched.\nagentId: a53fd635aaceeed5c (internal ID)"}])
    }

    fn delivered(id: &str, tool: &str) -> InsertedToolResult {
        InsertedToolResult {
            wire_tool_use_id: id.to_string(),
            tool_name: tool.to_string(),
        }
    }

    #[test]
    fn dispatch_ids_no_agent_result_yields_nothing_to_reconcile() {
        let results = vec![
            result(
                "toolu_read",
                json!([{"type": "text", "text": "file contents"}]),
            ),
            result("toolu_bash", json!("exit 0")),
            result("toolu_empty", json!(null)),
        ];
        let inserted = vec![
            delivered("toolu_read", "Read"),
            delivered("toolu_bash", "Bash"),
        ];
        assert!(dispatch_result_wire_ids(&results, &inserted).is_empty());
    }

    #[test]
    fn dispatch_ids_history_agent_result_not_newly_delivered_yields_nothing() {
        let results = vec![result("toolu_agent", agent_content())];
        assert!(dispatch_result_wire_ids(&results, &[]).is_empty());
    }

    #[test]
    fn dispatch_ids_bash_result_quoting_an_agent_id_yields_nothing() {
        let results = vec![result("toolu_bash", agent_content())];
        let inserted = vec![delivered("toolu_bash", "Bash")];
        assert!(dispatch_result_wire_ids(&results, &inserted).is_empty());
    }

    #[test]
    fn dispatch_ids_one_agent_result_yields_exactly_that_id() {
        let results = vec![
            result(
                "toolu_read",
                json!([{"type": "text", "text": "file contents"}]),
            ),
            result("toolu_agent", agent_content()),
            result("toolu_bash", json!("exit 0")),
        ];
        let inserted = vec![
            delivered("toolu_read", "Read"),
            delivered("toolu_agent", "Agent"),
            delivered("toolu_bash", "Bash"),
        ];
        assert_eq!(
            dispatch_result_wire_ids(&results, &inserted),
            vec!["toolu_agent"]
        );
    }

    #[test]
    fn dispatch_ids_spawn_session_payload_is_a_dispatch_result() {
        let payload = json!({"launch": {"session": "6dc7e09e-35ab-4ff5-bbc5-da10559c7bd7"}});
        let results = vec![
            result(
                "toolu_spawn_text",
                json!([{"type": "text", "text": payload.to_string()}]),
            ),
            result("toolu_spawn_string", json!(payload.to_string())),
            result("toolu_spawn_object", payload),
            result(
                "toolu_not_json",
                json!([{"type": "text", "text": "not JSON"}]),
            ),
        ];
        let inserted = vec![
            delivered("toolu_spawn_text", "mcp__mike__spawn_session"),
            delivered("toolu_spawn_string", "mcp__mike__spawn_session"),
            delivered("toolu_spawn_object", "spawn_session"),
            delivered("toolu_not_json", "mcp__mike__spawn_session"),
        ];
        assert_eq!(
            dispatch_result_wire_ids(&results, &inserted),
            vec![
                "toolu_spawn_text",
                "toolu_spawn_string",
                "toolu_spawn_object"
            ]
        );
    }

    #[test]
    fn dispatch_ids_empty_body_yields_nothing() {
        assert!(dispatch_result_wire_ids(&[], &[]).is_empty());
    }
}
