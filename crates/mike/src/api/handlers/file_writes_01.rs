use super::*;
use crate::store::persistence::web_repo::WebPersist as _;

pub(crate) const FILE_WRITES_DEFAULT_LIMIT: i64 = 100;
pub(crate) const FILE_WRITES_MAX_LIMIT: i64 = 500;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct FileWritesParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) from: Option<String>,
    #[serde(default)]
    pub(crate) to: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) collisions_only: bool,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FileWriteDto {
    pub(crate) session_id: String,
    pub(crate) request_id: i64,
    pub(crate) tool_name: String,
    pub(crate) created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subagent_persona: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line_delta: Option<i32>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FileWriteGroupDto {
    pub(crate) file_path: String,
    pub(crate) session_count: i64,
    pub(crate) write_count: i64,
    pub(crate) first_write: String,
    pub(crate) last_write: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) net_line_delta: Option<i64>,
    pub(crate) writes: Vec<FileWriteDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FileWritesResponse {
    pub(crate) total_count: i64,
    pub(crate) offset: i64,
    pub(crate) limit: i64,
    pub(crate) coverage: &'static str,
    pub(crate) files: Vec<FileWriteGroupDto>,
}

pub(crate) const FILE_WRITES_COVERAGE: &str = "Edit/Write/NotebookEdit calls only. Bash-mediated changes (git, shell \
     redirects, stash) carry no captured file path and are not represented. \
     write_count counts editing-tool calls, NOT changes: a file can show several writes \
     and be byte-identical, because a session edited and reverted. net_line_delta gives \
     lines added minus removed, where non-zero proves the file changed and zero does not \
     prove it did not (a revert nets to zero, as does any same-line-count substitution). \
     Neither number is a substitute for a diff when the answer matters.";

pub(crate) fn parse_bound(
    raw: Option<&String>,
    field: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, String> {
    match raw {
        None => Ok(None),
        Some(s) => chrono::DateTime::parse_from_rfc3339(&s.replace(' ', "+"))
            .map(|d| Some(d.with_timezone(&chrono::Utc)))
            .map_err(|e| format!("`{field}` must be RFC 3339: {e}")),
    }
}

pub(crate) async fn list_file_writes(
    State(state): State<AppState>,
    Query(params): Query<FileWritesParams>,
) -> Result<Json<FileWritesResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let offset = params.offset.unwrap_or(0).max(0);
    let limit = params
        .limit
        .unwrap_or(FILE_WRITES_DEFAULT_LIMIT)
        .clamp(1, FILE_WRITES_MAX_LIMIT);

    let filter = crate::store::persistence::file_writes::FileWriteFilter {
        project: params.project.as_deref(),
        from: parse_bound(params.from.as_ref(), "from")
            .map_err(|m| (StatusCode::BAD_REQUEST, m).into_response())?,
        to: parse_bound(params.to.as_ref(), "to")
            .map_err(|m| (StatusCode::BAD_REQUEST, m).into_response())?,
        path: params.path.as_deref(),
        collisions_only: params.collisions_only,
    };

    let query_failed = |e: &dyn std::fmt::Display| {
        tracing::warn!(error = %e, "list_file_writes: query failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "file_writes query failed",
        )
            .into_response()
    };

    let total_count = handle
        .web()
        .count_file_writes(&filter)
        .await
        .map_err(|e| query_failed(&e))?;
    let summaries = handle
        .web()
        .summarise_file_writes(&filter, offset, limit)
        .await
        .map_err(|e| query_failed(&e))?;

    let paths: Vec<String> = summaries.iter().map(|s| s.file_path.clone()).collect();
    let writes = handle
        .web()
        .fetch_writes_for_paths(&paths, &filter)
        .await
        .map_err(|e| query_failed(&e))?;

    use std::collections::HashMap;
    let mut by_path: HashMap<String, Vec<FileWriteDto>> = HashMap::with_capacity(paths.len());
    let mut net_deltas: HashMap<String, Option<i64>> = HashMap::with_capacity(paths.len());
    for w in writes {
        if let Some(delta) = w.line_delta {
            *net_deltas
                .entry(w.file_path.clone())
                .or_insert(None)
                .get_or_insert(0) += i64::from(delta);
        } else {
            net_deltas.entry(w.file_path.clone()).or_insert(None);
        }
        by_path.entry(w.file_path).or_default().push(FileWriteDto {
            session_id: w.session_uuid,
            request_id: w.request_id,
            tool_name: w.tool_name,
            created_at: w.created_at.to_rfc3339(),
            agent_name: w.agent_name,
            subagent_persona: w.subagent_persona,
            line_delta: w.line_delta,
        });
    }

    let files = summaries
        .into_iter()
        .map(|s| FileWriteGroupDto {
            writes: by_path.remove(&s.file_path).unwrap_or_default(),
            net_line_delta: net_deltas.remove(&s.file_path).flatten(),
            file_path: s.file_path,
            session_count: s.session_count,
            write_count: s.write_count,
            first_write: s.first_write.to_rfc3339(),
            last_write: s.last_write.to_rfc3339(),
        })
        .collect();

    Ok(Json(FileWritesResponse {
        total_count,
        offset,
        limit,
        coverage: FILE_WRITES_COVERAGE,
        files,
    }))
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReceiptConfidence {
    Acknowledged,
    High,
    Medium,
    None,
}
