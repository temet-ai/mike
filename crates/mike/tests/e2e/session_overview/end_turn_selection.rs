use crate::db_helpers;

use super::seeding::{
    InsertRequestParams, InsertTurnParams, bootstrap_project_session, insert_request,
    insert_response, insert_text_chunk, insert_turn, turn_by_id,
};

use serde_json::Value;
use wiremock::MockServer;

async fn seed_end_turn_scenario(
    pool: &sqlx::PgPool,
    session_id: &str,
    project_id: i64,
    long_prompt: &str,
) -> anyhow::Result<(i64, i64, i64, i64)> {
    let r1 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            status: "complete",
            human_prompt: long_prompt,
            completed: true,
            created_at_offset_ms: 0,
        },
    )
    .await?;
    insert_response(pool, r1, "tool_use").await?;
    insert_text_chunk(pool, r1, session_id, 0, "starting work").await?;

    let r2 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "tool_loop",
            status: "complete",
            human_prompt: "",
            completed: true,
            created_at_offset_ms: 1_000,
        },
    )
    .await?;
    insert_response(pool, r2, "tool_use").await?;
    insert_text_chunk(pool, r2, session_id, 0, "interstitial commentary").await?;

    let r3 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "tool_loop",
            status: "complete",
            human_prompt: "",
            completed: true,
            created_at_offset_ms: 2_000,
        },
    )
    .await?;
    insert_response(pool, r3, "end_turn").await?;
    insert_text_chunk(pool, r3, session_id, 0, "the final ").await?;
    insert_text_chunk(pool, r3, session_id, 1, "answer").await?;

    let t1 = insert_turn(
        pool,
        InsertTurnParams {
            session_id,
            project_id,
            start_request_id: r1,
            end_request_id: r3,
            user_prompt: Some(long_prompt),
            outcome: "accepted",
            closed: true,
        },
    )
    .await?;

    let r4 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            status: "pending",
            human_prompt: "next prompt",
            completed: false,
            created_at_offset_ms: 3_000,
        },
    )
    .await?;
    let t2 = insert_turn(
        pool,
        InsertTurnParams {
            session_id,
            project_id,
            start_request_id: r4,
            end_request_id: r4,
            user_prompt: Some("next prompt"),
            outcome: "unknown",
            closed: false,
        },
    )
    .await?;

    Ok((t1, r1, r3, t2))
}

fn assert_closed_turn_shows_end_turn_text_only(
    overview: &Value,
    t1: i64,
    r1: i64,
    r3: i64,
    long_prompt: &str,
) -> anyhow::Result<()> {
    let row1 = turn_by_id(overview, t1)?;
    assert_eq!(row1["stop_request_id"], r3);
    assert_eq!(
        row1["stop_response"], "the final answer",
        "stop_response must be the end_turn request's chunks in seq_ord order, nothing else"
    );
    assert_eq!(row1["stop_reason"], "end_turn");
    assert_eq!(row1["interrupted"], false);
    assert_eq!(row1["outcome"], "accepted");
    assert_eq!(row1["user_prompt"], long_prompt);
    assert_eq!(row1["start_request_id"], r1);
    assert_eq!(row1["end_request_id"], r3);
    assert_eq!(row1["start_request_number"], 1);
    assert_eq!(row1["end_request_number"], 3);
    assert!(
        row1["closed_at"].is_string(),
        "closed turn must carry closed_at: {row1}"
    );
    Ok(())
}

fn assert_open_turn_has_no_stop_fields(overview: &Value, t2: i64) -> anyhow::Result<()> {
    let row2 = turn_by_id(overview, t2)?;
    assert_eq!(row2["stop_response"], Value::Null);
    assert_eq!(row2["stop_reason"], Value::Null);
    assert_eq!(row2["stop_request_id"], Value::Null);
    assert_eq!(row2["interrupted"], false);
    assert_eq!(row2["closed_at"], Value::Null);
    Ok(())
}

#[tokio::test]
async fn overview_picks_end_turn_request_text_only() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "0a000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "overview-endturn", session_id).await?;

    let long_prompt = "implement the overview: ".to_string() + &"y".repeat(3_000);

    let (t1, r1, r3, t2) =
        seed_end_turn_scenario(&pool, session_id, project_id, &long_prompt).await?;

    let client = reqwest::Client::new();
    let url = format!("{inspect_url}/api/sessions/{session_id}/overview");
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let overview: Value = resp.json().await?;

    assert_eq!(overview["session_id"], session_id);
    assert_eq!(overview["total_turns"], 2);
    assert_eq!(overview["truncated"], false);
    let turns = overview["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("turns array missing: {overview}"))?;
    assert_eq!(turns.len(), 2);

    assert_eq!(turns[0]["turn_id"], t2);
    assert_eq!(turns[1]["turn_id"], t1);

    assert_closed_turn_shows_end_turn_text_only(&overview, t1, r1, r3, &long_prompt)?;
    assert_open_turn_has_no_stop_fields(&overview, t2)?;

    let asc: Value = client
        .get(format!("{url}?order=asc"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(asc["turns"][0]["turn_id"], t1);
    assert_eq!(asc["turns"][1]["turn_id"], t2);

    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/0a000000-0000-4000-8000-0000000000ff/overview"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    let resp = client
        .get(format!("{inspect_url}/api/sessions/not-a-uuid/overview"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
