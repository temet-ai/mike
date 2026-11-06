use sqlx::PgPool;

pub(super) struct Seeded {
    pub(super) session_uuid: String,
    pub(super) wire_tool_use_id: String,
}

async fn seed_dims_and_bodies(
    pool: &PgPool,
    session_uuid: &str,
) -> anyhow::Result<(i64, i64, i64)> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name)
         VALUES ('/home/user/demo', 'demo') RETURNING id",
    )
    .fetch_one(pool)
    .await?;

    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, title, last_status)
         VALUES ($1::uuid, $2, 'demo session', 'complete') RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    let message_id: i64 = sqlx::query_scalar(
        "INSERT INTO messages.messages (message_sha256, body, byte_size, role_id)
         VALUES (repeat('a', 64), '{\"role\":\"user\",\"content\":\"hi\"}'::jsonb, 30, 1)
         RETURNING id",
    )
    .fetch_one(pool)
    .await?;

    let _sysprompt_id: i64 = sqlx::query_scalar(
        "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks)
         VALUES (repeat('b', 64), '[]'::jsonb) RETURNING id",
    )
    .fetch_one(pool)
    .await?;

    Ok((project_id, session_id, message_id))
}

async fn seed_requests(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    message_id: i64,
) -> anyhow::Result<(i64, i64)> {
    let req1: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
            (session_id, project_id, method_id, path, model, is_streaming,
             message_count, is_tool_loop, status_id, params, last_user_message,
             human_prompt, turn_type_id, turn_origin_id, request_body)
         VALUES ($1, $2, 2, '/v1/messages', 'claude-x', true, 1, false, 3,
                 '{}'::jsonb, 'hi', 'hi', 1, 1, '\\xdeadbeef'::bytea)
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    let req2: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
            (session_id, project_id, method_id, path, model, is_streaming,
             message_count, is_tool_loop, status_id, params, last_user_message,
             human_prompt, turn_type_id, turn_origin_id)
         VALUES ($1, $2, 2, '/v1/messages', 'claude-x', true, 2, true, 3,
                 '{}'::jsonb, 'hi', 'hi', 2, 1)
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    sqlx::query("INSERT INTO requests.messages (request_id, message_id, ord) VALUES ($1, $2, 0)")
        .bind(req1)
        .bind(message_id)
        .execute(pool)
        .await?;

    Ok((req1, req2))
}

async fn seed_response_and_stream(pool: &PgPool, req1: i64, session_id: i64) -> anyhow::Result<()> {
    let _resp_id: i64 = sqlx::query_scalar(
        "INSERT INTO responses.responses
            (request_id, response_status, token_usage, completed_at)
         VALUES ($1, 200, '{\"input_tokens\":\"10\",\"output_tokens\":\"5\"}'::jsonb, now())
         RETURNING id",
    )
    .bind(req1)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO responses.stream_blocks
            (request_id, session_id, block_type_id, seq_ord, content,
             signature, wire_tool_use_id, content_sha256, byte_size)
         VALUES ($1, $2, 1, 0, 'hello', NULL, NULL, repeat('c', 64), 5)",
    )
    .bind(req1)
    .bind(session_id)
    .execute(pool)
    .await?;

    Ok(())
}

async fn seed_tool_use_and_result(
    pool: &PgPool,
    req1: i64,
    req2: i64,
    session_id: i64,
    wire_tool_use_id: &str,
) -> anyhow::Result<i64> {
    let tool_use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ($1, $2, 0, 'Bash', '{\"command\":\"ls\"}'::jsonb, $3) RETURNING id",
    )
    .bind(wire_tool_use_id)
    .bind(req1)
    .bind(session_id)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO tools.results
            (request_id, tool_use_id, session_id, ord, is_error, content, content_sha256, content_bytes)
         VALUES ($1, $2, $3, 0, false, '\"output\"'::jsonb, repeat('d', 64), 6)",
    )
    .bind(req2)
    .bind(tool_use_id)
    .bind(session_id)
    .execute(pool)
    .await?;

    Ok(tool_use_id)
}

async fn seed_operator_turn(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    req1: i64,
    req2: i64,
) -> anyhow::Result<()> {
    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
            (session_id, project_id, start_request_id, end_request_id, user_prompt)
         VALUES ($1, $2, $3, $4, 'hi') RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(req1)
    .bind(req2)
    .fetch_one(pool)
    .await?;

    sqlx::query("UPDATE requests.requests SET operator_turn_id = $1 WHERE id IN ($2, $3)")
        .bind(turn_id)
        .bind(req1)
        .bind(req2)
        .execute(pool)
        .await?;

    Ok(())
}

pub(super) async fn seed_source(pool: &PgPool) -> anyhow::Result<Seeded> {
    let session_uuid = "5e550000-0000-4000-8000-000000000001".to_string();
    let wire_tool_use_id = "toolu_resync_demo_0001".to_string();

    let (project_id, session_id, message_id) = seed_dims_and_bodies(pool, &session_uuid).await?;
    let (req1, req2) = seed_requests(pool, session_id, project_id, message_id).await?;
    seed_response_and_stream(pool, req1, session_id).await?;
    seed_tool_use_and_result(pool, req1, req2, session_id, &wire_tool_use_id).await?;
    seed_operator_turn(pool, session_id, project_id, req1, req2).await?;

    Ok(Seeded {
        session_uuid,
        wire_tool_use_id,
    })
}

pub(super) const SEEDED_TABLES: &[(&str, i64)] = &[
    ("projects.projects", 1),
    ("sessions.sessions", 1),
    ("messages.messages", 1),
    ("requests.requests", 2),
    ("requests.messages", 1),
    ("responses.responses", 1),
    ("responses.stream_blocks", 1),
    ("tools.uses", 1),
    ("tools.results", 1),
    ("memory.operator_turns", 1),
];
