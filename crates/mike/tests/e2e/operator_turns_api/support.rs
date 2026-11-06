use serde_json::json;
use sqlx::PgPool;

pub(crate) struct InsertRequestParams<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) project_id: i64,
    pub(crate) turn_origin: &'a str,
    pub(crate) turn_type: &'a str,
    pub(crate) human_prompt: &'a str,
    pub(crate) created_at_offset_ms: i64,
}

pub(crate) async fn insert_request(
    pool: &PgPool,
    p: InsertRequestParams<'_>,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(p.created_at_offset_ms);
    let empty_obj = json!({});
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
                $2, rm.id, '/v1/messages', false, 0, false,
                rs.id, $3,
                '', $4, rtt.id, rto.id,
                $7, $7
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = $5 AND rto.code = $6
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(&empty_obj)
    .bind(p.human_prompt)
    .bind(p.turn_type)
    .bind(p.turn_origin)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(crate) async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, $2, $2
         WHERE NOT EXISTS (
             SELECT 1 FROM projects.projects WHERE project_path = $1
         )",
    )
    .bind(project_path)
    .bind(now)
    .execute(pool)
    .await?;
    let project_id: i64 = sqlx::query_scalar(
        "SELECT id FROM projects.projects WHERE project_path = $1 ORDER BY id LIMIT 1",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(project_id)
}

pub(crate) struct InsertOperatorTurnParams<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) project_id: i64,
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) user_prompt: &'a str,
    pub(crate) outcome: &'a str,
    pub(crate) outcome_evidence: Option<&'a str>,
}

pub(crate) async fn insert_operator_turn(
    pool: &PgPool,
    p: InsertOperatorTurnParams<'_>,
) -> anyhow::Result<i64> {
    sqlx::query(
        "UPDATE requests.requests
            SET last_user_message = $1
          WHERE id = $2",
    )
    .bind(p.user_prompt)
    .bind(p.start_request_id)
    .execute(pool)
    .await?;

    let evidence_pair: Option<(i64, chrono::DateTime<chrono::Utc>)> = match p.outcome_evidence {
        None => None,
        Some(text) => {
            let row: (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
                "INSERT INTO requests.requests (
                    session_id, project_id, method_id, path, is_streaming,
                    message_count, is_tool_loop, status_id,
                    params,
                    last_user_message, human_prompt, turn_type_id, turn_origin_id,
                    created_at, completed_at
                 )
                 SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
                        $2, rm.id, '/v1/messages', false,
                        0, false, rs.id,
                        '{}'::jsonb,
                        $3, '', rtt.id, rto.id,
                        NOW(), NOW()
                 FROM requests.methods rm, requests.statuses rs,
                      requests.turn_types rtt, requests.turn_origins rto
                 WHERE rm.code = 'POST' AND rs.code = 'complete'
                   AND rtt.code = 'user_prompt' AND rto.code = 'primary'
                 RETURNING id, created_at",
            )
            .bind(p.session_id)
            .bind(p.project_id)
            .bind(text)
            .fetch_one(pool)
            .await?;
            Some(row)
        }
    };

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            outcome_id, classified_at, closed_at,
            outcome_evidence_request_id
         )
         SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid), $2,
                $3, $4,
                toc.id, NOW(), NOW(),
                $6
         FROM memory.turn_outcomes toc
         WHERE toc.code = $5
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(p.start_request_id)
    .bind(p.end_request_id)
    .bind(p.outcome)
    .bind(evidence_pair.map(|(id, _)| id))
    .fetch_one(pool)
    .await?;
    Ok(id)
}
