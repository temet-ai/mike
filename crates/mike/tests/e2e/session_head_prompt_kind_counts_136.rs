use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::MockServer;

async fn bootstrap_project_session(
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

struct InsertRequestParams<'a> {
    session_id: &'a str,
    project_id: i64,
    human_prompt: &'a str,
    created_at_offset_ms: i64,
}

async fn insert_request(pool: &PgPool, p: InsertRequestParams<'_>) -> anyhow::Result<i64> {
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
                $5, $5
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(&empty_obj)
    .bind(p.human_prompt)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

struct InsertTurnParams<'a> {
    session_id: &'a str,
    project_id: i64,
    start_request_id: i64,
    user_prompt: &'a str,
}

async fn insert_turn(pool: &PgPool, p: InsertTurnParams<'_>) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            user_prompt, outcome_id, classified_at, closed_at
         )
         SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid), $2,
                $3, $3,
                $4, toc.id, NOW(), NOW()
         FROM memory.turn_outcomes toc
         WHERE toc.code = 'accepted'
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(p.start_request_id)
    .bind(p.user_prompt)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn insert_classification(
    pool: &PgPool,
    request_id: i64,
    session_id: &str,
    kind_code: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
         SELECT $1,
                (SELECT id FROM sessions.sessions WHERE session_uuid = $2::text::uuid),
                pk.id
           FROM prompt_kinds.kinds pk
          WHERE pk.code = $3
         ON CONFLICT (request_id) DO NOTHING",
    )
    .bind(request_id)
    .bind(session_id)
    .bind(kind_code)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn session_head_prompt_kind_counts_match_seeded_distribution() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "7a000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "head-136-mixed", session_id).await?;

    let seeds = [
        ("do the work", "operator"),
        ("fix the bug", "operator"),
        (
            "Describe your most recent action in 3-5 words: testing",
            "activity-label",
        ),
        ("Stop hook feedback: lint failed", "hook-feedback"),
    ];
    for (i, (prompt, kind)) in seeds.iter().enumerate() {
        let r = insert_request(
            &pool,
            InsertRequestParams {
                session_id,
                project_id,
                human_prompt: prompt,
                created_at_offset_ms: (i as i64) * 1_000,
            },
        )
        .await?;
        insert_turn(
            &pool,
            InsertTurnParams {
                session_id,
                project_id,
                start_request_id: r,
                user_prompt: prompt,
            },
        )
        .await?;
        insert_classification(&pool, r, session_id, kind).await?;
    }

    let client = reqwest::Client::new();
    let head: Value = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/head"))
        .send()
        .await?
        .json()
        .await?;

    let counts = head["prompt_kind_counts"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("missing prompt_kind_counts object: {head}"))?;
    assert_eq!(
        counts.len(),
        17,
        "all 17 known kind codes must be present: {counts:?}"
    );
    assert_eq!(counts["operator"], 2, "counts: {counts:?}");
    assert_eq!(counts["activity-label"], 1, "counts: {counts:?}");
    assert_eq!(counts["hook-feedback"], 1, "counts: {counts:?}");
    for kind in [
        "teammate-message",
        "session-recap",
        "background-event",
        "compaction",
        "analysis-probe",
        "empty-prompt",
    ] {
        assert_eq!(
            counts[kind], 0,
            "unseeded kind must be present at 0, not absent: {counts:?}"
        );
    }

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_head_prompt_kind_counts_all_zero_for_empty_session() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "7a000000-0000-4000-8000-000000000002";
    bootstrap_project_session(&pool, "head-136-empty", session_id).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/head"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let head: Value = resp.json().await?;

    let counts = head["prompt_kind_counts"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("missing prompt_kind_counts object: {head}"))?;
    assert_eq!(counts.len(), 17, "all 17 known kind codes: {counts:?}");
    for kind in [
        "operator",
        "activity-label",
        "hook-feedback",
        "teammate-message",
        "session-recap",
        "background-event",
        "compaction",
        "analysis-probe",
        "empty-prompt",
    ] {
        assert_eq!(
            counts[kind], 0,
            "kind {kind} must be present at 0 for a turn-less session: {counts:?}"
        );
    }

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
