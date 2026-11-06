use crate::db_helpers;

use super::seeding::{
    InsertRequestParams, InsertTurnParams, bootstrap_project_session, insert_request,
    insert_text_chunk, insert_turn, turn_by_id,
};

use serde_json::Value;
use wiremock::MockServer;

#[tokio::test]
async fn overview_span_fallback_covers_unstamped_aborted_request() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "0b000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "overview-aborted", session_id).await?;

    let r1 = insert_request(
        &pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            status: "complete",
            human_prompt: "do the risky thing",
            completed: true,
            created_at_offset_ms: 0,
        },
    )
    .await?;

    let r2 = insert_request(
        &pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "tool_loop",
            status: "error",
            human_prompt: "",
            completed: true,
            created_at_offset_ms: 1_000,
        },
    )
    .await?;
    insert_text_chunk(&pool, r2, session_id, 0, "partial answer ").await?;
    insert_text_chunk(&pool, r2, session_id, 1, "before abort").await?;

    let t1 = insert_turn(
        &pool,
        InsertTurnParams {
            session_id,
            project_id,
            start_request_id: r1,
            end_request_id: r1,
            user_prompt: None,
            outcome: "abandoned",
            closed: true,
        },
    )
    .await?;

    let client = reqwest::Client::new();
    let overview: Value = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/overview"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(overview["total_turns"], 1);

    let row = turn_by_id(&overview, t1)?;
    assert_eq!(
        row["stop_request_id"], r2,
        "span fallback must select the unstamped aborted request: {row}"
    );
    assert_eq!(row["stop_response"], "partial answer before abort");
    assert_eq!(
        row["stop_reason"], "aborted",
        "no settled responses row -> stop_reason 'aborted'"
    );
    assert_eq!(row["interrupted"], true);
    assert_eq!(row["outcome"], "abandoned");
    assert_eq!(
        row["user_prompt"], "do the risky thing",
        "turn.user_prompt is NULL -> falls back to the start request's human_prompt"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
