use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

pub(super) struct Seeded {
    pub(super) session: Uuid,
    pub(super) project_id: i64,
    pub(super) session_id: i64,
}

pub(super) async fn seed_session(pool: &PgPool, project_path: &str) -> anyhow::Result<Seeded> {
    let session = Uuid::new_v4();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name)
         VALUES ($1, $1)
         RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now() - interval '2 hours', now() - interval '1 hour')
         RETURNING id",
    )
    .bind(session)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(Seeded {
        session,
        project_id,
        session_id,
    })
}

pub(super) async fn insert_request(
    pool: &PgPool,
    seeded: &Seeded,
    manifest: Option<Value>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
            (session_id, project_id, method_id, path, model, is_streaming, message_count,
             is_tool_loop, status_id, params, last_user_message, human_prompt, turn_type_id,
             turn_origin_id, transformation_manifest)
         SELECT $1, $2, m.id, '/v1/messages', 'claude-opus-test', false, 1, false, st.id,
                '{}'::jsonb, '', 'delivery fixture prompt', ttype.id, torg.id, $3
           FROM requests.methods m, requests.turn_origins torg,
                requests.turn_types ttype, requests.statuses st
          WHERE m.code = 'POST' AND torg.code = 'primary' AND ttype.code = 'user_prompt'
            AND st.code = 'complete'
         RETURNING id",
    )
    .bind(seeded.session_id)
    .bind(seeded.project_id)
    .bind(manifest)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) fn manifest(stages: Vec<Value>) -> Value {
    json!({
        "schema_version": 1,
        "original_sha256": "a",
        "sent_sha256": "b",
        "stages": stages,
    })
}

pub(super) fn stage(name: &str, outcome: &str, reason: &str, params: Value) -> Value {
    json!({
        "ordinal": 1,
        "stage": name,
        "outcome": outcome,
        "policy_id": name,
        "policy_version": 1,
        "resolved_parameters": params,
        "input_sha256": "a",
        "output_sha256": "b",
        "targets": [],
        "reason_code": reason,
        "detail": null,
    })
}

pub(super) async fn insert_lesson(
    pool: &PgPool,
    project_id: Option<i64>,
    text: &str,
    topic: Option<&str>,
    stage: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, $2, ls.id, lsrc.id, $3
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'convention' AND ls.code = $4 AND lsrc.code = 'operator'
         RETURNING id",
    )
    .bind(project_id)
    .bind(text)
    .bind(topic)
    .bind(stage)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn archive_lesson(pool: &PgPool, lesson_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.lessons l
            SET stage_id = s.id, archived_at = now(), archived_reason_id = ar.id
           FROM memory.lesson_stages s, memory.lesson_archived_reasons ar
          WHERE s.code = 'archived' AND ar.code = 'operator' AND l.id = $1",
    )
    .bind(lesson_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn inject_lesson(
    pool: &PgPool,
    seeded: &Seeded,
    lesson_id: i64,
    request_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lesson_injections (lesson_id, request_id, session_id, lane_id, persona)
         SELECT $1, $2, $3, lane.id, 'Rust Development Expert'
           FROM memory.lesson_injection_lanes lane
          WHERE lane.code = 'primary'",
    )
    .bind(lesson_id)
    .bind(request_id)
    .bind(seeded.session_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn insert_tool_use(
    pool: &PgPool,
    seeded: &Seeded,
    request_id: i64,
    ord: i32,
    tool: (&str, Value, bool),
) -> anyhow::Result<()> {
    let (tool_name, input, is_error) = tool;
    let tool_use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(format!("toolu_{}", Uuid::new_v4().simple()))
    .bind(request_id)
    .bind(ord)
    .bind(tool_name)
    .bind(input)
    .bind(seeded.session_id)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO tools.results
            (request_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes, session_id)
         VALUES ($1, $2, $3, $4, '[]'::jsonb, $5, 2, $6)",
    )
    .bind(request_id)
    .bind(tool_use_id)
    .bind(ord)
    .bind(is_error)
    .bind(format!("sha-{tool_use_id}"))
    .bind(seeded.session_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn log_toggle_change(
    pool: &PgPool,
    key: &str,
    before: bool,
    after: bool,
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM config.change_log WHERE subject_kind = 'toggle' AND subject_key = $1")
        .bind(key)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO config.change_log (subject_kind, subject_key, op, old_value, new_value, via)
         VALUES ('toggle', $1, 'update', $2, $3, 'spa')",
    )
    .bind(key)
    .bind(json!({"enabled": before, "operator_set": true}))
    .bind(json!({"enabled": after, "operator_set": true}))
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn seed_lessons_and_topics(
    pool: &PgPool,
    seeded: &Seeded,
    first_request: i64,
) -> anyhow::Result<()> {
    let project = Some(seeded.project_id);
    let delivered =
        insert_lesson(pool, project, "Use scripts/cx for cargo.", None, "daily").await?;
    inject_lesson(pool, seeded, delivered, first_request).await?;
    insert_lesson(pool, project, "sqlx lesson one", Some("sqlx"), "long_term").await?;
    insert_lesson(pool, None, "shared sqlx lesson", Some("sqlx"), "daily").await?;
    insert_lesson(pool, project, "tokio lesson", Some("tokio"), "daily").await?;
    let archived =
        insert_lesson(pool, project, "archived axum lesson", Some("axum"), "daily").await?;
    archive_lesson(pool, archived).await
}

pub(super) async fn seed_delivery_story(pool: &PgPool) -> anyhow::Result<Seeded> {
    let seeded = seed_session(pool, "/work/delivery-api").await?;
    let first = insert_request(
        pool,
        &seeded,
        Some(manifest(vec![
            stage(
                "session_lessons",
                "applied",
                "session_lessons_injected",
                json!({"reminder_text_len": 120}),
            ),
            stage("recall_injector", "unchanged", "recall_no_tags", json!({})),
        ])),
    )
    .await?;
    let second = insert_request(
        pool,
        &seeded,
        Some(manifest(vec![stage(
            "recall_injector",
            "applied",
            "recall_lessons_injected",
            json!({"topics": ["sqlx"], "lesson_count": 2}),
        )])),
    )
    .await?;
    insert_request(pool, &seeded, None).await?;
    seed_lessons_and_topics(pool, &seeded, first).await?;

    insert_tool_use(
        pool,
        &seeded,
        second,
        0,
        ("Skill", json!({"skill": "rust-lsp"}), false),
    )
    .await?;
    insert_tool_use(
        pool,
        &seeded,
        second,
        1,
        ("Skill", json!({"skill": "missing"}), true),
    )
    .await?;
    insert_tool_use(
        pool,
        &seeded,
        second,
        2,
        (
            "mcp__mike__recall_project_context",
            json!({"query": "sqlx"}),
            false,
        ),
    )
    .await?;
    log_toggle_change(pool, "recall_injector", false, true).await?;
    Ok(seeded)
}
