use std::time::Duration;

use crate::anthropic_fixtures;
use crate::db_helpers;

use super::seeding::{
    InsertRequestParams, InsertTurnParams, bootstrap_project_session, insert_request, insert_turn,
    turn_by_id,
};
use super::sse_fixtures::truncated_sse_body;

use anthropic_fixtures::{
    RequestBody, assistant_message, tool_result_block, tool_use_block, user_message,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type ClosedTurnRow = (
    i64,
    Option<chrono::DateTime<chrono::Utc>>,
    String,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<String>,
);

async fn seed_open_turn_anchor(
    pool: &sqlx::PgPool,
    session_id: &str,
    project_id: i64,
) -> anyhow::Result<(i64, i64)> {
    let r1 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            status: "complete",
            human_prompt: "fix the bug",
            completed: true,
            created_at_offset_ms: -2_000,
        },
    )
    .await?;
    let t1 = insert_turn(
        pool,
        InsertTurnParams {
            session_id,
            project_id,
            start_request_id: r1,
            end_request_id: r1,
            user_prompt: Some("fix the bug"),
            outcome: "unknown",
            closed: false,
        },
    )
    .await?;
    Ok((r1, t1))
}

async fn send_streaming_tool_loop_continuation(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(
            &RequestBody::default()
                .messages(json!([
                    user_message("fix the bug"),
                    assistant_message(json!([tool_use_block("toolu_abort_1", "Bash", json!({}))])),
                    user_message(json!([tool_result_block("toolu_abort_1", "ok")])),
                ]))
                .stream(true)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await?;
    Ok(())
}

async fn wait_for_aborted_request(
    pool: &sqlx::PgPool,
    excluding_request_id: i64,
    session_id: &str,
) -> anyhow::Result<i64> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT r.id
               FROM requests.requests r
               JOIN requests.statuses st ON st.id = r.status_id
               JOIN requests.errors   re ON re.request_id = r.id
              WHERE r.id <> $1
                AND r.session_id = (SELECT id FROM sessions.sessions
                                     WHERE session_uuid = $2::text::uuid)
                AND st.code = 'error'
                AND re.message = 'stream aborted'",
        )
        .bind(excluding_request_id)
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
        if let Some((id,)) = row {
            return Ok(id);
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "timed out waiting for the aborted request row to settle as error"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn assert_turn_stamped_by_abort(pool: &sqlx::PgPool, t1: i64, r2: i64) -> anyhow::Result<()> {
    let stamped_turn: Option<i64> =
        sqlx::query_scalar("SELECT operator_turn_id FROM requests.requests WHERE id = $1")
            .bind(r2)
            .fetch_one(pool)
            .await?;
    assert_eq!(
        stamped_turn,
        Some(t1),
        "aborted request must be stamped into the turn it closes"
    );

    let (end_request_id, closed_at, outcome, classified_at, final_text): ClosedTurnRow =
        sqlx::query_as(
        "SELECT t.end_request_id, t.closed_at, toc.code, t.classified_at, t.final_assistant_text
           FROM memory.operator_turns t
           JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
          WHERE t.id = $1",
    )
    .bind(t1)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        end_request_id, r2,
        "abort must advance the closed turn's end_request_id to the aborted request"
    );
    assert!(closed_at.is_some(), "turn must be closed by the abort");
    assert_eq!(
        outcome, "unknown",
        "a dropped stream closes the turn for the judge to rule on, not as abandoned"
    );
    assert!(
        classified_at.is_none(),
        "the closed turn must stay claimable by the judge"
    );
    assert_eq!(
        final_text.as_deref(),
        Some("partial one partial two"),
        "materialized final_assistant_text must include the partial pre-abort chunks"
    );
    Ok(())
}

async fn assert_overview_shows_abort(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: &str,
    t1: i64,
    r2: i64,
) -> anyhow::Result<()> {
    let overview: Value = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/overview"))
        .send()
        .await?
        .json()
        .await?;
    let row = turn_by_id(&overview, t1)?;
    assert_eq!(row["stop_request_id"], r2);
    assert_eq!(row["stop_response"], "partial one partial two");
    assert_eq!(row["stop_reason"], "aborted");
    assert_eq!(row["interrupted"], true);
    assert_eq!(row["outcome"], "unknown");
    Ok(())
}

#[tokio::test]
async fn aborted_stream_closes_turn_unjudged_and_materialized() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let sse_body = truncated_sse_body(&["partial one ", "partial two"]);
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
        )
        .mount(&mock)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "0c000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "overview-abort-stamp", session_id).await?;

    let (r1, t1) = seed_open_turn_anchor(&pool, session_id, project_id).await?;

    let client = reqwest::Client::new();
    send_streaming_tool_loop_continuation(&client, &proxy_url, session_id).await?;

    let r2 = wait_for_aborted_request(&pool, r1, session_id).await?;

    assert_turn_stamped_by_abort(&pool, t1, r2).await?;
    assert_overview_shows_abort(&client, &inspect_url, session_id, t1, r2).await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
