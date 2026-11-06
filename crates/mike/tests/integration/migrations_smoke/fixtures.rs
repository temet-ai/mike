use sqlx::Row;

pub(super) const EXPECTED_INDEXES: &[(&str, &str)] = &[
    ("requests", "idx_request_diagnostics_category_created_at"),
    ("requests", "idx_request_diagnostics_analyzer_created_at"),
    ("requests", "idx_requests_turn_origin_created_at"),
    ("requests", "idx_requests_model_created_at"),
    ("responses", "idx_request_responses_upstream_request_id"),
    ("requests", "idx_requests_status_created_at"),
    ("sessions", "idx_sessions_title_trgm"),
    ("requests", "idx_request_tools_request"),
    ("memory", "idx_otc_chunk_text_trgm"),
    ("memory", "idx_operator_turns_search_tsv"),
    ("requests", "idx_rsp_firing_rules_request_system_prompt_id"),
];

pub(super) async fn insert_request_with_diagnostics(
    pool: &sqlx::PgPool,
    _diagnostics: &serde_json::Value,
) -> anyhow::Result<(i64, chrono::DateTime<chrono::Utc>)> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/smoke-{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now();

    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects
            (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(&project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         RETURNING id",
    )
    .bind(&session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let row = sqlx::query(
        "INSERT INTO requests.requests (
            session_id, project_id,
            method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id,
            created_at
         )
         SELECT
            $1, $2,
            rm.id, '/v1/messages', false, 1, false,
            rs.id, '{}'::jsonb,
            'hello', 'hello',
            rtt.id, rto.id,
            $3
         FROM requests.methods rm,
              requests.statuses rs,
              requests.turn_types rtt,
              requests.turn_origins rto
         WHERE rm.code = 'POST'
           AND rs.code = 'complete'
           AND rtt.code = 'user_prompt'
           AND rto.code = 'primary'
         RETURNING id, created_at",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let request_id: i64 = row.get(0);
    let request_created_at: chrono::DateTime<chrono::Utc> = row.get(1);

    Ok((request_id, request_created_at))
}

pub(super) async fn insert_request_diagnostics(
    pool: &sqlx::PgPool,
    request_id: i64,
    _request_created_at: chrono::DateTime<chrono::Utc>,
    diagnostics: &serde_json::Value,
) -> anyhow::Result<()> {
    if let Some(arr) = diagnostics.as_array() {
        for d in arr {
            sqlx::query(
                "INSERT INTO requests.diagnostics (
                    request_id, category, analyzer,
                    advice, matched_text, match_range, other_hits, created_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
                 ON CONFLICT DO NOTHING",
            )
            .bind(request_id)
            .bind(
                d.get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
            )
            .bind(
                d.get("analyzer")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
            )
            .bind(d.get("advice").and_then(|v| v.as_str()).unwrap_or(""))
            .bind(d.get("matched_text").and_then(|v| v.as_str()).unwrap_or(""))
            .bind(d.get("match_range"))
            .bind(
                d.get("other_hits")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            )
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}
