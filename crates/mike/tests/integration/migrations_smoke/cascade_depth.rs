use crate::test_db;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

struct CascadeFixtureIds {
    session_uuid: String,
    session_pk: i64,
    request_id: i64,
    agent_use_pk: i64,
    agent_pk: i64,
    turn_id: i64,
}

struct SessionRequestSeed {
    session_uuid: String,
    session_pk: i64,
    project_id: i64,
    request_id: i64,
}

async fn seed_session_and_request(pool: &PgPool) -> anyhow::Result<SessionRequestSeed> {
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/cascade-{}", uuid::Uuid::new_v4());
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
    .bind(&session_uuid)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
            (session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id, created_at,
             params,
             last_user_message, human_prompt, turn_type_id, turn_origin_id)
         SELECT $1, $2, rm.id, '/v1/messages', FALSE,
                1, FALSE, rs.id, $3,
                '{}'::jsonb,
                'hi', 'hi', rtt.id, rto.id
         FROM requests.methods rm,
              requests.statuses rs,
              requests.turn_types rtt,
              requests.turn_origins rto
         WHERE rm.code = 'POST'
           AND rs.code = 'complete'
           AND rtt.code = 'user_prompt'
           AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    Ok(SessionRequestSeed {
        session_uuid,
        session_pk,
        project_id,
        request_id,
    })
}

async fn seed_tool_activity(
    pool: &PgPool,
    request_id: i64,
    session_pk: i64,
) -> anyhow::Result<(i64, i64, i64)> {
    let bash_wire_id = format!("toolu_bash_{}", uuid::Uuid::new_v4());
    let agent_wire_id = format!("toolu_agent_{}", uuid::Uuid::new_v4());
    let bash_use_pk: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ($1, $2, 0, 'Bash', $3, $4)
         RETURNING id",
    )
    .bind(&bash_wire_id)
    .bind(request_id)
    .bind(serde_json::json!({"command": "echo hi"}))
    .bind(session_pk)
    .fetch_one(pool)
    .await?;
    let agent_use_pk: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ($1, $2, 1, 'Agent', $3, $4)
         RETURNING id",
    )
    .bind(&agent_wire_id)
    .bind(request_id)
    .bind(serde_json::json!({"subagent_type": "rust-developer"}))
    .bind(session_pk)
    .fetch_one(pool)
    .await?;

    let agent_name = format!("rust-developer-{}", uuid::Uuid::new_v4());
    let agent_pk: i64 = sqlx::query_scalar(
        "INSERT INTO agents.agents (name, definition_path)
         VALUES ($1, '.claude/agents/rust-developer.md')
         RETURNING id",
    )
    .bind(&agent_name)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id)
         VALUES ($1, $2)",
    )
    .bind(agent_use_pk)
    .bind(agent_pk)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO tools.results
            (request_id, ord, tool_use_id, session_id,
             is_error, content, content_sha256, content_bytes)
         VALUES ($1, 0, $2, $3, FALSE, $4, $5, $6)",
    )
    .bind(request_id)
    .bind(bash_use_pk)
    .bind(session_pk)
    .bind(serde_json::json!("hi\n"))
    .bind("a".repeat(64))
    .bind(3_i64)
    .execute(pool)
    .await?;

    Ok((bash_use_pk, agent_use_pk, agent_pk))
}

async fn seed_diagnostic_and_turn(
    pool: &PgPool,
    request_id: i64,
    session_pk: i64,
    project_id: i64,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO requests.diagnostics
            (request_id, category, analyzer, advice,
             matched_text, match_range, other_hits)
         VALUES ($1, 'weasel', 'weasel-pattern', 'rephrase',
                 'maybe', '{\"start\":0,\"end\":5}'::jsonb, '[]'::jsonb)",
    )
    .bind(request_id)
    .execute(pool)
    .await?;

    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
            (session_id, project_id, start_request_id, end_request_id,
             closed_at)
         VALUES ($1, $2, $3, $3, $4)
         RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(request_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let zero_vec = format!("[{}]", vec!["0"; 768].join(","));
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text,
             chunk_embedding, embedding_model)
         SELECT $1, 0, cr.id, 'hi', $2::vector, 'cascade-test-fixture'
         FROM memory.chunk_roles cr
         WHERE cr.code = 'prompt'",
    )
    .bind(turn_id)
    .bind(&zero_vec)
    .execute(pool)
    .await?;

    Ok(turn_id)
}

async fn seed_cascade_fixture(pool: &PgPool) -> anyhow::Result<CascadeFixtureIds> {
    let seed = seed_session_and_request(pool).await?;
    let (_bash_use_pk, agent_use_pk, agent_pk) =
        seed_tool_activity(pool, seed.request_id, seed.session_pk).await?;
    let turn_id =
        seed_diagnostic_and_turn(pool, seed.request_id, seed.session_pk, seed.project_id).await?;

    Ok(CascadeFixtureIds {
        session_uuid: seed.session_uuid,
        session_pk: seed.session_pk,
        request_id: seed.request_id,
        agent_use_pk,
        agent_pk,
        turn_id,
    })
}

type CascadeCounts = (i64, i64, i64, i64, i64, i64, i64);

async fn count_cascade_targets(
    pool: &PgPool,
    ids: &CascadeFixtureIds,
) -> anyhow::Result<CascadeCounts> {
    let counts = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM requests.requests           WHERE session_id = $1),
            (SELECT COUNT(*) FROM tools.uses          WHERE request_id = $2),
            (SELECT COUNT(*) FROM tools.results       WHERE request_id = $2),
            (SELECT COUNT(*) FROM agents.dispatches   WHERE tool_use_id = $3),
            (SELECT COUNT(*) FROM requests.diagnostics WHERE request_id = $2),
            (SELECT COUNT(*) FROM memory.operator_turns     WHERE session_id = $1),
            (SELECT COUNT(*) FROM memory.operator_turn_chunks WHERE turn_id  = $4)",
    )
    .bind(ids.session_pk)
    .bind(ids.request_id)
    .bind(ids.agent_use_pk)
    .bind(ids.turn_id)
    .fetch_one(pool)
    .await?;
    Ok(counts)
}

#[tokio::test]
async fn cascade_full_depth_after_all_migrations() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping full-depth cascade test");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let ids = seed_cascade_fixture(&pool).await?;

    let pre_counts = count_cascade_targets(&pool, &ids).await?;
    assert_eq!(
        pre_counts,
        (1, 2, 1, 1, 1, 1, 1),
        "fixture must seed every cascade target before the parent DELETE"
    );

    let deleted = sqlx::query("DELETE FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
        .bind(&ids.session_uuid)
        .execute(&pool)
        .await?;
    assert_eq!(deleted.rows_affected(), 1, "exactly one session deleted");

    let post_counts = count_cascade_targets(&pool, &ids).await?;
    assert_eq!(
        post_counts,
        (0, 0, 0, 0, 0, 0, 0),
        "every cascade target row must be removed when the parent session is deleted"
    );

    let agent_still_present: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agents.agents WHERE id = $1")
            .bind(ids.agent_pk)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        agent_still_present, 1,
        "agents row must survive its dispatch's cascade — agents are reused across sessions"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
