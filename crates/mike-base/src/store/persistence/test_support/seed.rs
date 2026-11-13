use sqlx::PgPool;

pub async fn seed_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
        project_path,
    )
    .fetch_one(pool)
    .await?;
    Ok(project_id)
}

pub async fn seed_two_sessions(
    pool: &PgPool,
    session_a: &str,
    session_b: &str,
    project_id: i64,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $3, NOW(), NOW()), ($2::text::uuid, $3, NOW(), NOW())",
        session_a,
        session_b,
        project_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn seed_session_turns(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
    count: i64,
) -> anyhow::Result<()> {
    let request_ids: Vec<i64> = sqlx::query_scalar!(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id,
            params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT ss.id, $2, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                NOW() + (g.i || ' microseconds')::interval,
                NOW() + (g.i || ' microseconds')::interval
         FROM generate_series(1::bigint, $3::bigint) AS g(i),
              sessions.sessions ss,
              requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE ss.session_uuid = $1::text::uuid
           AND rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
        session_id,
        project_id,
        count,
    )
    .fetch_all(pool)
    .await?;
    anyhow::ensure!(request_ids.len() as i64 == count, "seed sanity");

    let ts_pairs = sqlx::query!(
        "SELECT id, created_at
           FROM requests.requests
          WHERE id = ANY($1)
          ORDER BY id ASC",
        &request_ids,
    )
    .fetch_all(pool)
    .await?;
    anyhow::ensure!(ts_pairs.len() as i64 == count, "ts pair sanity");

    let ids: Vec<i64> = ts_pairs.iter().map(|row| row.id).collect();
    let tss: Vec<chrono::DateTime<chrono::Utc>> =
        ts_pairs.iter().map(|row| row.created_at).collect();
    sqlx::query!(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            outcome_id, classified_at, closed_at, created_at
         )
         SELECT ss.id, $2, u.rid, u.rid, tot.id, NOW(), NOW(), u.rts
         FROM UNNEST($3::bigint[], $4::timestamptz[]) AS u(rid, rts),
              sessions.sessions ss,
              memory.turn_outcomes tot
         WHERE ss.session_uuid = $1::text::uuid
           AND tot.code = 'unknown'",
        session_id,
        project_id,
        &ids,
        &tss,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn seed_captured_request(
    pool: &PgPool,
    session_uuid: &str,
    project_id: i64,
    turn_origin_code: &str,
    request_body: &[u8],
) -> anyhow::Result<i64> {
    let session_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())
         RETURNING id",
        session_uuid,
        project_id,
    )
    .fetch_one(pool)
    .await?;

    let request_id: i64 = sqlx::query_scalar!(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id,
             turn_origin_id, request_body, created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false,
                1, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                $3, NOW(), NOW()
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = $4
          RETURNING id",
        session_id,
        project_id,
        request_body,
        turn_origin_code,
    )
    .fetch_one(pool)
    .await?;

    Ok(request_id)
}

pub async fn request_row_count(pool: &PgPool, session_id: &str) -> anyhow::Result<i64> {
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!"
           FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::text::uuid"#,
        session_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub async fn pin_session_row(pool: &PgPool, session_id: &str) -> anyhow::Result<()> {
    sqlx::query!(
        "UPDATE sessions.sessions SET pinned = true WHERE session_uuid = $1::text::uuid",
        session_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn pin_project_for_session(pool: &PgPool, session_id: &str) -> anyhow::Result<()> {
    sqlx::query!(
        "UPDATE projects.projects SET pinned = true
         WHERE id IN (
             SELECT project_id FROM sessions.sessions WHERE session_uuid = $1::text::uuid
         )",
        session_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn session_pinned(pool: &PgPool, session_id: &str) -> anyhow::Result<bool> {
    let pinned = sqlx::query_scalar!(
        "SELECT pinned FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
        session_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(pinned)
}
