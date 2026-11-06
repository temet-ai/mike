use super::*;
use crate::store::persistence::test_support;
use serde_json::json;

fn three_block_system(role_body: &str) -> JsonValue {
    json!([
        {"type": "text", "text": "billing-header"},
        {"type": "text", "text": "identity preamble", "cache_control": {"type": "ephemeral"}},
        {"type": "text", "text": role_body, "cache_control": {"type": "ephemeral", "ttl": "1h"}},
    ])
}

async fn seed_unresolved_request(pool: &sqlx::PgPool, body: &JsonValue) -> anyhow::Result<i64> {
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/sysprompt-backfill-{}", uuid::Uuid::new_v4());
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

    let session_id: i64 = sqlx::query_scalar(
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

    let body_bytes = serde_json::to_vec(body)?;
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            request_body, created_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false, 1, false, rs.id,
                '{}'::jsonb, '', '', rtt.id, rto.id, $3, $4
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(&body_bytes)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query!(
        "INSERT INTO requests.system_prompts (request_id) VALUES ($1)",
        request_id,
    )
    .execute(pool)
    .await?;

    Ok(request_id)
}

#[tokio::test]
async fn backfill_resolves_canonical_shape() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let body = json!({"system": three_block_system("You are Claude Code."), "messages": []});
    let request_id = seed_unresolved_request(&handle.pool, &body).await?;

    let outcome = handle.backfill_request_system_prompts(100).await?;
    assert_eq!(outcome.resolved, 1);
    assert_eq!(outcome.unknown_shape, 0);

    let (sha, system_prompt_id): (Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT prompt_sha256, system_prompt_id FROM requests.system_prompts WHERE request_id = $1",
    )
    .bind(request_id)
    .fetch_one(&handle.pool)
    .await?;
    let expected_sha = prompt_sha256_from_block_text("You are Claude Code.");
    assert_eq!(sha, Some(expected_sha.clone()));
    assert!(system_prompt_id.is_some());

    let stored_blocks: JsonValue = sqlx::query_scalar(
        "SELECT blocks FROM system_prompts.system_prompts WHERE prompt_sha256 = $1",
    )
    .bind(&expected_sha)
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(
        stored_blocks,
        three_block_system("You are Claude Code."),
        "the FULL verbatim system array (cache_control included) must be stored, matching proxy/engine.rs's system_blocks_value"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_dedupes_identical_role_body_across_requests() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let baseline_prompts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM system_prompts.system_prompts")
            .fetch_one(&handle.pool)
            .await?;

    let body = json!({"system": three_block_system("shared identity"), "messages": []});
    let r1 = seed_unresolved_request(&handle.pool, &body).await?;
    let r2 = seed_unresolved_request(&handle.pool, &body).await?;

    let outcome = handle.backfill_request_system_prompts(100).await?;
    assert_eq!(outcome.resolved, 2);

    let ids: Vec<Option<i64>> = sqlx::query_scalar(
        "SELECT system_prompt_id FROM requests.system_prompts
          WHERE request_id = ANY($1) ORDER BY request_id",
    )
    .bind([r1, r2])
    .fetch_all(&handle.pool)
    .await?;
    assert_eq!(ids.len(), 2);
    assert_eq!(
        ids[0], ids[1],
        "identical role-body text must dedupe to one system_prompt_id"
    );

    let distinct_prompts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM system_prompts.system_prompts")
            .fetch_one(&handle.pool)
            .await?;
    assert_eq!(
        distinct_prompts,
        baseline_prompts + 1,
        "exactly one new canonical row for the shared prompt"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_records_unknown_shape_when_nothing_is_hashable() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let body = json!({
        "system": [{"type": "text", "text": "x-anthropic-billing-header: cch=abc;"}],
        "messages": []
    });
    let request_id = seed_unresolved_request(&handle.pool, &body).await?;

    let outcome = handle.backfill_request_system_prompts(100).await?;
    assert_eq!(outcome.resolved, 0);
    assert_eq!(outcome.unknown_shape, 1);

    let (sha, system_prompt_id, unknown_blocks): (Option<String>, Option<i64>, Option<JsonValue>) =
        sqlx::query_as(
            "SELECT prompt_sha256, system_prompt_id, unknown_blocks
               FROM requests.system_prompts WHERE request_id = $1",
        )
        .bind(request_id)
        .fetch_one(&handle.pool)
        .await?;
    assert_eq!(sha, None);
    assert_eq!(system_prompt_id, None);
    assert_eq!(
        unknown_blocks,
        Some(json!({"reason": "no_identity_block", "len": 1}))
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_resolves_the_four_block_interactive_shape() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let body = json!({
        "system": [
            {"type": "text", "text": "x-anthropic-billing-header: cch=abc;"},
            {"type": "text", "text": "You are Claude Code."},
            {"type": "text", "text": "harness prompt"},
            {"type": "text", "text": "the role body"},
        ],
        "messages": []
    });
    let request_id = seed_unresolved_request(&handle.pool, &body).await?;

    let outcome = handle.backfill_request_system_prompts(100).await?;
    assert_eq!(outcome.resolved, 1);

    let sha: Option<String> = sqlx::query_scalar(
        "SELECT prompt_sha256 FROM requests.system_prompts WHERE request_id = $1",
    )
    .bind(request_id)
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(sha, Some(prompt_sha256_from_block_text("the role body")));

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reclassify_resolves_previously_unknown_rows_only() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let four_block = json!({
        "system": [
            {"type": "text", "text": "x-anthropic-billing-header: cch=abc;"},
            {"type": "text", "text": "You are Claude Code."},
            {"type": "text", "text": "harness prompt"},
            {"type": "text", "text": "recoverable role body"},
        ],
        "messages": []
    });
    let recoverable = seed_unresolved_request(&handle.pool, &four_block).await?;
    let header_only = json!({
        "system": [{"type": "text", "text": "x-anthropic-billing-header: cch=abc;"}],
        "messages": []
    });
    let unrecoverable = seed_unresolved_request(&handle.pool, &header_only).await?;

    sqlx::query!(
        "UPDATE requests.system_prompts
            SET unknown_blocks = '{\"reason\": \"non_3_block\", \"len\": 4}'::jsonb
          WHERE request_id = $1",
        recoverable,
    )
    .execute(&handle.pool)
    .await?;
    sqlx::query!(
        "UPDATE requests.system_prompts
            SET unknown_blocks = '{\"reason\": \"non_3_block\", \"len\": 1}'::jsonb
          WHERE request_id = $1",
        unrecoverable,
    )
    .execute(&handle.pool)
    .await?;

    let outcome = handle.reclassify_unknown_shape_system_prompts(100).await?;
    assert_eq!(outcome.resolved, 1);
    assert_eq!(outcome.unknown_shape, 1);

    let (sha, unknown): (Option<String>, Option<JsonValue>) = sqlx::query_as(
        "SELECT prompt_sha256, unknown_blocks FROM requests.system_prompts WHERE request_id = $1",
    )
    .bind(recoverable)
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(
        sha,
        Some(prompt_sha256_from_block_text("recoverable role body"))
    );
    assert_eq!(unknown, None);

    let still_unknown: Option<JsonValue> = sqlx::query_scalar(
        "SELECT unknown_blocks FROM requests.system_prompts WHERE request_id = $1",
    )
    .bind(unrecoverable)
    .fetch_one(&handle.pool)
    .await?;
    assert!(still_unknown.is_some());

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_skips_rows_with_no_body_and_terminates() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let body = json!({"system": three_block_system("irrelevant"), "messages": []});
    let request_id = seed_unresolved_request(&handle.pool, &body).await?;
    sqlx::query!(
        "UPDATE requests.requests SET request_body = NULL WHERE id = $1",
        request_id,
    )
    .execute(&handle.pool)
    .await?;

    let outcome = handle.backfill_request_system_prompts(100).await?;
    assert_eq!(outcome.resolved, 0);
    assert_eq!(outcome.unknown_shape, 0);
    assert_eq!(outcome.batches, 0, "an empty first batch must not loop");

    let missing = handle
        .count_system_prompts_missing_and_uncompactable()
        .await?;
    assert_eq!(
        missing, 1,
        "the no-body row must surface in the separate accepted-loss count"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_is_a_noop_on_rerun() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let baseline_prompts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM system_prompts.system_prompts")
            .fetch_one(&handle.pool)
            .await?;

    let body = json!({"system": three_block_system("idempotent"), "messages": []});
    seed_unresolved_request(&handle.pool, &body).await?;

    let first = handle.backfill_request_system_prompts(100).await?;
    assert_eq!(first.resolved, 1);

    let second = handle.backfill_request_system_prompts(100).await?;
    assert_eq!(second.resolved, 0);
    assert_eq!(second.unknown_shape, 0);
    assert_eq!(second.batches, 0);

    let distinct_prompts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM system_prompts.system_prompts")
            .fetch_one(&handle.pool)
            .await?;
    assert_eq!(
        distinct_prompts,
        baseline_prompts + 1,
        "no duplicate canonical row from the re-run"
    );

    db.cleanup().await?;
    Ok(())
}
