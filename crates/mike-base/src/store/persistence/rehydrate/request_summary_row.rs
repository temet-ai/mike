use std::collections::HashMap;

use serde::Deserialize;
use sqlx::PgPool;

use super::row::normalize_status_code;
use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

#[derive(Debug, Deserialize)]
struct ParamsMaxTokensOnly {
    max_tokens: Option<u32>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct RequestSummaryRow {
    pub session_id: String,
    pub id: i64,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub is_streaming: bool,
    pub message_count: i32,
    pub status: String,
    pub params: serde_json::Value,
    pub human_prompt: String,
    pub turn_type: String,
    pub turn_origin: String,
    pub subagent_persona: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub prompt_sha256: Option<String>,
    pub composition_id: Option<i64>,
    pub predecessor_request_id: Option<i64>,
    pub project_path: String,
    pub request_number: Option<i64>,
    pub agent_instance_id: Option<i64>,
    pub dispatch_request_id: Option<i64>,
}

pub fn row_to_request_summary(
    row: RequestSummaryRow,
    declared: Vec<String>,
    invoked_names: Vec<String>,
) -> Result<crate::store::request_summary::RequestSummaryView, PersistError> {
    let request_id = if row.id < 0 {
        return Err(PersistError::DataInvariant(format!(
            "requests.id must be non-negative, got {}",
            row.id
        )));
    } else {
        row.id as u64
    };

    let message_count = if row.message_count < 0 {
        0usize
    } else {
        row.message_count as usize
    };

    let tool_names: Vec<String> = declared;

    let response_tool_count = invoked_names.len();
    let mut response_tools_used: Vec<String> = Vec::new();
    for name in invoked_names {
        if !response_tools_used.iter().any(|existing| existing == &name) {
            response_tools_used.push(name);
        }
    }

    let params: ParamsMaxTokensOnly = serde_json::from_value(row.params).map_err(|e| {
        PersistError::DataInvariant(format!(
            "failed to decode params during summary rehydrate: {e}"
        ))
    })?;

    Ok(crate::store::request_summary::RequestSummaryView {
        id: request_id,
        session_id: row.session_id,
        method: row.method,
        path: row.path,
        model: row.model,
        status: normalize_status_code(&row.status),
        is_streaming: row.is_streaming,
        message_count,
        max_tokens: params.max_tokens,
        created_at: row.created_at.to_rfc3339(),
        completed_at: row.completed_at.map(|t| t.to_rfc3339()),
        project: if row.project_path == crate::store::persistence::SENTINEL_PROJECT_PATH {
            None
        } else {
            Some(row.project_path)
        },
        git_repo: None,
        human_prompt: row.human_prompt,
        turn_type: row.turn_type,
        turn_origin: row.turn_origin,
        subagent_persona: row.subagent_persona,
        tool_names,
        response_tool_count,
        response_tools_used,
        prompt_sha256: row.prompt_sha256,
        prompt_composition_id: row.composition_id.map(|u| u.to_string()),
        predecessor_request_id: row.predecessor_request_id,
        request_number: row.request_number.map(|n| n.max(0) as u64),
        agent_instance_id: row.agent_instance_id,
        dispatch_request_id: row.dispatch_request_id,
    })
}

pub async fn fetch_tool_names_for_keys(
    pool: &PgPool,
    keys: &[i64],
) -> Result<(HashMap<i64, Vec<String>>, HashMap<i64, Vec<String>>), PersistError> {
    if keys.is_empty() {
        return Ok((HashMap::new(), HashMap::new()));
    }

    let declared_rows = sqlx::query!(
        "SELECT rt.request_id, rt.tool_name
           FROM requests.tools rt
          WHERE rt.request_id = ANY($1)
          ORDER BY rt.request_id, rt.ord",
        keys,
    )
    .fetch_all(pool)
    .await?;

    let invoked_rows = sqlx::query!(
        "SELECT tu.request_id, tu.tool_name
           FROM tools.uses tu
          WHERE tu.request_id = ANY($1)
          ORDER BY tu.request_id, tu.ord",
        keys,
    )
    .fetch_all(pool)
    .await?;

    let mut declared_by_key: HashMap<i64, Vec<String>> = HashMap::new();
    for r in declared_rows {
        declared_by_key
            .entry(r.request_id)
            .or_default()
            .push(r.tool_name);
    }
    let mut invoked_by_key: HashMap<i64, Vec<String>> = HashMap::new();
    for r in invoked_rows {
        invoked_by_key
            .entry(r.request_id)
            .or_default()
            .push(r.tool_name);
    }

    Ok((declared_by_key, invoked_by_key))
}
