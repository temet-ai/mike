use crate::test_db;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

async fn seed_turn(pool: &sqlx::PgPool, prompt: &str) -> anyhow::Result<(i64, i64)> {
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/stop-harness-turns-{}", uuid::Uuid::new_v4());
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

    let request_id: i64 = sqlx::query_scalar(
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
            $3, $3,
            (SELECT id FROM requests.turn_types WHERE code = 'user_prompt' LIMIT 1),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary' LIMIT 1),
            $4
         )
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(prompt)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
            (session_id, project_id, start_request_id, end_request_id, user_prompt)
         VALUES ($1, $2, $3, $3, $4)
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(request_id)
    .bind(prompt)
    .fetch_one(pool)
    .await?;

    sqlx::query!(
        "UPDATE requests.requests SET operator_turn_id = $1 WHERE id = $2",
        turn_id,
        request_id,
    )
    .execute(pool)
    .await?;

    Ok((request_id, turn_id))
}

const STOP_HARNESS_TURNS_DELETE: &str = "
    DELETE FROM memory.operator_turns ot
     WHERE ot.user_prompt IS NULL
        OR ot.user_prompt = ''
        OR ot.user_prompt LIKE '%Describe your most recent action in 3-5 words%'
        OR ot.user_prompt LIKE '%Stop hook feedback:%'
        OR ot.user_prompt LIKE '%<teammate-message%'
        OR ot.user_prompt LIKE '%<agent-message from=%'
        OR ot.user_prompt LIKE '%The user stepped away and is coming back%'
        OR ot.user_prompt LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
        OR ot.user_prompt LIKE '%This session is being continued from a previous conversation%'
        OR ot.user_prompt LIKE '%Summarize this portion of a Claude Code session transcript%'
        OR ot.user_prompt LIKE '%Analyze this Claude Code session and extract structured facets%'
        OR ot.user_prompt LIKE '%has the following stopping condition been satisfied%'
";

#[tokio::test]
async fn stop_harness_turns_migration_applies_on_fresh_db() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping stop-harness-turns smoke");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory.operator_turns")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0, "fresh DB has no operator_turns rows to clean up");

    let empty_prompt_kind: Option<String> =
        sqlx::query_scalar("SELECT code FROM prompt_kinds.kinds WHERE id = 9")
            .fetch_optional(&pool)
            .await?;
    assert_eq!(
        empty_prompt_kind.as_deref(),
        Some("empty-prompt"),
        "prompt_kinds.kinds must carry the #104 empty-prompt kind (id 9)"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn stop_harness_turns_deletes_matching_and_spares_genuine() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping stop-harness-turns seeded-state test");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let harness_prompts = [
        "Describe your most recent action in 3-5 words",
        "Stop hook feedback: lint clean",
        "<teammate-message from=\"x\">hi</teammate-message>",
        "<agent-message from=\"y\">hi</agent-message>",
        "The user stepped away and is coming back. Recap in under 40 words.",
        "[SYSTEM NOTIFICATION - NOT USER INPUT]\nbackground event",
        "This session is being continued from a previous conversation.",
        "Summarize this portion of a Claude Code session transcript.",
        "Analyze this Claude Code session and extract structured facets.",
        "has the following stopping condition been satisfied",
        "",
    ];

    let mut harness_turn_ids = Vec::new();
    let mut fk_request_id = None;
    for prompt in harness_prompts {
        let (request_id, turn_id) = seed_turn(&pool, prompt).await?;
        harness_turn_ids.push(turn_id);
        if prompt.is_empty() {
            fk_request_id = Some(request_id);
        }
    }
    let (_genuine_request_id, genuine_turn_id) =
        seed_turn(&pool, "Fix the flaky retry test").await?;

    let pre_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory.operator_turns")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        pre_count,
        harness_prompts.len() as i64 + 1,
        "every seeded row must exist before the delete"
    );

    sqlx::query(STOP_HARNESS_TURNS_DELETE)
        .execute(&pool)
        .await?;

    for turn_id in &harness_turn_ids {
        let survives: Option<i64> =
            sqlx::query_scalar("SELECT id FROM memory.operator_turns WHERE id = $1")
                .bind(turn_id)
                .fetch_optional(&pool)
                .await?;
        assert!(
            survives.is_none(),
            "harness-pattern turn {turn_id} must be deleted"
        );
    }

    let genuine_survives: Option<i64> =
        sqlx::query_scalar("SELECT id FROM memory.operator_turns WHERE id = $1")
            .bind(genuine_turn_id)
            .fetch_optional(&pool)
            .await?;
    assert_eq!(
        genuine_survives,
        Some(genuine_turn_id),
        "the genuine operator turn must survive the delete"
    );

    let fk_request_id: i64 = fk_request_id.context("empty-prompt row was seeded")?;
    let (request_still_exists, stamped_turn): (bool, Option<i64>) = {
        let row: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT operator_turn_id FROM requests.requests WHERE id = $1")
                .bind(fk_request_id)
                .fetch_optional(&pool)
                .await?;
        match row {
            Some((stamped,)) => (true, stamped),
            None => (false, None),
        }
    };
    assert!(
        request_still_exists,
        "ON DELETE SET NULL must not cascade-delete the referencing request row"
    );
    assert_eq!(
        stamped_turn, None,
        "the request's operator_turn_id must be nulled, not left dangling"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
