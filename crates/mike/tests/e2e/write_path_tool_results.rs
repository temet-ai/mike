use crate::db_helpers;

use crate::upstream;

use crate::anthropic_fixtures;

use anthropic_fixtures::{RequestBody, assistant_message, tool_use_block, user_message};
use db_helpers::TestDb;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

struct SeededRequestContext {
    session_pk: i64,
    request_id: i64,
    request_created_at: chrono::DateTime<chrono::Utc>,
}

async fn seed_request_context(pool: &PgPool) -> anyhow::Result<SeededRequestContext> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ('c7-fk-test', 'c7-fk-test', $1, $1)
         RETURNING id",
    )
    .bind(now)
    .fetch_one(pool)
    .await?;
    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ('c7000000-0000-4000-8000-000000000001'::uuid, $1, $2, $2)
         RETURNING id",
    )
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let (request_id, request_created_at): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            (SELECT id FROM sessions.sessions WHERE session_uuid = 'c7000000-0000-4000-8000-000000000001'::uuid),
            $1, 2, '/v1/messages', false, 0, false,
            3, '{}'::jsonb,
            '', '', 1, 1,
            $2
         ) RETURNING id, created_at",
    )
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    Ok(SeededRequestContext {
        session_pk,
        request_id,
        request_created_at,
    })
}

async fn assert_orphan_tool_result_insert_rejected(
    pool: &PgPool,
    ctx: &SeededRequestContext,
) -> anyhow::Result<()> {
    let result = sqlx::query(
        "INSERT INTO tools.results (
            request_id, ord, tool_use_id, session_id,
            is_error, content, content_sha256, content_bytes, created_at
         ) VALUES ($1, 0, 999999999, $3, false,
                   '\"x\"'::jsonb, 'sha-c7', 1, $2)",
    )
    .bind(ctx.request_id)
    .bind(ctx.request_created_at)
    .bind(ctx.session_pk)
    .execute(pool)
    .await;

    let err = match result {
        Ok(_) => {
            anyhow::bail!(
                "FK on tool_use_id must reject the orphan INSERT, but the INSERT succeeded"
            )
        }
        Err(e) => e,
    };
    let msg = format!("{err}");
    let sqlstate = err
        .as_database_error()
        .and_then(|d| d.code().map(|c| c.into_owned()))
        .unwrap_or_default();
    assert_eq!(
        sqlstate, "23503",
        "expected SQLSTATE 23503 (foreign_key_violation), got {sqlstate}; full error: {msg}"
    );
    assert!(
        msg.contains("tool_use_id") || msg.contains("tool_uses"),
        "FK error must reference tool_use_id / tool_uses; got: {msg}"
    );
    Ok(())
}

async fn insert_tool_use_and_matching_result(
    pool: &PgPool,
    ctx: &SeededRequestContext,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, created_at, session_id)
         VALUES ('toolu_unknown_c7_fk', $1, 0, 'Bash', '{}'::jsonb, $2, $3)",
    )
    .bind(ctx.request_id)
    .bind(ctx.request_created_at)
    .bind(ctx.session_pk)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO tools.results (
            request_id, ord, tool_use_id, session_id,
            is_error, content, content_sha256, content_bytes, created_at
         ) VALUES ($1, 0,
                   (SELECT id FROM tools.uses WHERE wire_tool_use_id = 'toolu_unknown_c7_fk'),
                   $3,
                   false, '\"x\"'::jsonb, 'sha-c7', 1, $2)",
    )
    .bind(ctx.request_id)
    .bind(ctx.request_created_at)
    .bind(ctx.session_pk)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn tool_results_fk_to_tool_uses_enforced() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping write_path_tool_results");
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let ctx = seed_request_context(&pool).await?;
    assert_orphan_tool_result_insert_rejected(&pool, &ctx).await?;

    insert_tool_use_and_matching_result(&pool, &ctx).await?;

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

async fn seed_emitting_request_only_in_db(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
    tool_id: &str,
    tool_name: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, $2, $2
         WHERE NOT EXISTS (SELECT 1 FROM projects.projects WHERE project_path = $1)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_optional(pool)
    .await?
    .unwrap_or(0i64);
    let project_id: i64 = if project_id == 0 {
        sqlx::query_scalar("SELECT id FROM projects.projects WHERE project_path = $1")
            .bind(project_path)
            .fetch_one(pool)
            .await?
    } else {
        project_id
    };

    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;

    let empty_obj = json!({});
    let (request_id, _request_created_at): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2, 2, '/v1/messages', false, 0, false,
            3, $3,
            '', '', 1, 1,
            $4
         ) RETURNING id, created_at",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ($1, $2, 0, $3, '{}'::jsonb,
                 (SELECT session_id FROM requests.requests WHERE id = $2))
         ON CONFLICT (wire_tool_use_id) DO NOTHING",
    )
    .bind(tool_id)
    .bind(request_id)
    .bind(tool_name)
    .execute(pool)
    .await?;

    Ok(request_id)
}

fn carrying_body_with_tool_result(
    session_id: &str,
    tool_use_id: &str,
    is_error: bool,
    content: Value,
) -> Value {
    let mut body = RequestBody::default()
        .messages(json!([
            user_message("do the thing"),
            assistant_message(json!([tool_use_block(tool_use_id, "Bash", json!({}))])),
            user_message(json!([{
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "is_error": is_error,
                "content": content
            }])),
        ]))
        .build();
    body["session_id_hint"] = json!(session_id);
    body
}

async fn wait_for_carrying_request(
    pool: &PgPool,
    session_id: &str,
    excluded_id: i64,
) -> anyhow::Result<i64> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT r.id FROM requests.requests r
              JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid
                AND r.id <> $2
              ORDER BY r.created_at DESC
              LIMIT 1",
        )
        .bind(session_id)
        .bind(excluded_id)
        .fetch_optional(pool)
        .await?;
        if let Some((id,)) = row {
            return Ok(id);
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("timed out waiting for carrying request to be persisted");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

async fn assert_tool_results_table_empty(pool: &PgPool) -> anyhow::Result<()> {
    let pre_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tools.results")
        .fetch_one(pool)
        .await?;
    assert_eq!(
        pre_count.0, 0,
        "pre-condition: tool_results must start empty for this test"
    );
    Ok(())
}

async fn drive_carrying_request_through_proxy(
    proxy_url: &str,
    session_id: &str,
    tool_use_id: &str,
    content: Value,
) -> anyhow::Result<()> {
    let carrying_body = carrying_body_with_tool_result(session_id, tool_use_id, false, content);
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(&carrying_body)
        .send()
        .await?;
    assert!(
        resp.status().is_success(),
        "proxy roundtrip for carrying request failed: {}",
        resp.status()
    );
    Ok(())
}

async fn assert_carrying_result_row(
    pool: &PgPool,
    carrying_id: i64,
    emitting_id: i64,
    tool_use_id: &str,
    expected_content: &Value,
) -> anyhow::Result<()> {
    let rows: Vec<(i64, String, bool, Value, String, i64)> = sqlx::query_as(
        "SELECT tr.request_id, tu.wire_tool_use_id, tr.is_error, tr.content, tr.content_sha256, tr.content_bytes
           FROM tools.results tr
           JOIN tools.uses tu ON tu.id = tr.tool_use_id
          ORDER BY tr.request_id, tr.ord",
    )
    .fetch_all(pool)
    .await?;
    assert_eq!(
        rows.len(),
        1,
        "expected exactly one tool_results row (the carrying-request INSERT); got {} rows",
        rows.len()
    );
    let (row_request_id, row_tool_use_id, row_is_error, row_content, row_sha, row_bytes) = &rows[0];

    assert_eq!(
        *row_request_id, carrying_id,
        "tool_results.request_id must point at the CARRYING request (B), not the emitter (A); \
         carrying_id={carrying_id}, emitting_id={emitting_id}, got request_id={row_request_id}"
    );
    assert_ne!(
        *row_request_id, emitting_id,
        "tool_results.request_id must NOT be the emitting request — that was the pre-fix \
         behaviour and is incorrect per IMPLEMENTATION.md §4 step 3"
    );
    assert_eq!(row_tool_use_id, tool_use_id);
    assert!(!row_is_error);
    assert_eq!(
        row_content, expected_content,
        "content must round-trip byte-identical (inspection-fidelity)"
    );
    assert_eq!(
        row_sha.len(),
        64,
        "content_sha256 must be a 64-char hex string"
    );
    assert!(*row_bytes > 0, "content_bytes must be populated");
    Ok(())
}

#[tokio::test]
async fn tool_results_inserted_in_carrying_request() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_c7").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping carrying-request regression");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655442001";
    let project_path = "test-project-c7-carrying";
    let tool_use_id = "toolu_c7_carrying_001";

    let emitting_id =
        seed_emitting_request_only_in_db(&pool, session_id, project_path, tool_use_id, "Bash")
            .await?;

    assert_tool_results_table_empty(&pool).await?;

    let result_content = json!("carrying-request body content that must round-trip");
    drive_carrying_request_through_proxy(
        &proxy_url,
        session_id,
        tool_use_id,
        result_content.clone(),
    )
    .await?;
    let carrying_id = wait_for_carrying_request(&pool, session_id, emitting_id).await?;
    assert_ne!(
        carrying_id, emitting_id,
        "B's id must differ from A's; the proxy assigns a fresh id per intercept"
    );

    assert_carrying_result_row(
        &pool,
        carrying_id,
        emitting_id,
        tool_use_id,
        &result_content,
    )
    .await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
