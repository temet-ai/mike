pub(super) async fn insert_bridge_row(
    pool: &sqlx::PgPool,
    request_id: i64,
    ord: i32,
    sha: &str,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO requests.messages
            (request_id, ord, message_id)
         SELECT $1, $2, m.id
           FROM messages.messages m
          WHERE m.message_sha256 = $3",
        request_id,
        ord,
        sha,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn insert_request(
    pool: &sqlx::PgPool,
) -> anyhow::Result<(i64, chrono::DateTime<chrono::Utc>)> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/msgdedup-{}", uuid::Uuid::new_v4());
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

    let rec = sqlx::query!(
        "INSERT INTO requests.requests (
            session_id, project_id,
            method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id,
            created_at
         )
         VALUES (
            $1, $2,
            (SELECT id FROM requests.methods WHERE code = 'POST' LIMIT 1),
            '/v1/messages', false, 1, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete' LIMIT 1),
            '{}'::jsonb,
            'hello', 'hello',
            (SELECT id FROM requests.turn_types WHERE code = 'user_prompt' LIMIT 1),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary' LIMIT 1),
            $3
         )
         RETURNING id, created_at",
        session_pk,
        project_id,
        now,
    )
    .fetch_one(pool)
    .await?;
    Ok((rec.id, rec.created_at))
}

pub(super) fn sha_placeholder() -> String {
    let simple = uuid::Uuid::new_v4().simple().to_string();
    let s = format!("{simple}{simple}");
    debug_assert_eq!(s.len(), 64);
    s
}
