use crate::db_helpers;

use super::support::{
    InsertOperatorTurnParams, InsertRequestParams, bootstrap_project_session, insert_operator_turn,
    insert_request,
};
use sqlx::PgPool;

async fn seed_two_turns(
    pool: &PgPool,
    project_id: i64,
    session_id: &str,
    long_prompt: &str,
) -> anyhow::Result<(i64, i64, i64)> {
    let r1 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: long_prompt,
            created_at_offset_ms: 0,
        },
    )
    .await?;
    let r2 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "no, redo it",
            created_at_offset_ms: 2_000,
        },
    )
    .await?;

    let turn1_id = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: r1,
            end_request_id: r1,
            user_prompt: long_prompt,
            outcome: "corrected",
            outcome_evidence: Some("no, redo it"),
        },
    )
    .await?;
    let _turn2_id = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: r2,
            end_request_id: r2,
            user_prompt: "no, redo it",
            outcome: "unknown",
            outcome_evidence: None,
        },
    )
    .await?;

    Ok((r1, r2, turn1_id))
}

async fn assert_session_turns_default_order(
    client: &reqwest::Client,
    url: &str,
    r1: i64,
    r2: i64,
    long_prompt: &str,
) -> anyhow::Result<()> {
    let resp = client.get(url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json().await?;
    assert_eq!(body.len(), 2);
    let row1 = body
        .iter()
        .find(|t| t["start_request_id"] == r1)
        .ok_or_else(|| anyhow::anyhow!("turn 1 row not found in {body:?}"))?;
    assert_eq!(row1["user_prompt"], long_prompt);
    assert_eq!(row1["outcome"], "corrected");
    assert_eq!(row1["outcome_evidence"], "no, redo it");

    assert_eq!(
        body[0]["start_request_id"], r2,
        "default order must be newest-first (desc by start_request_id)"
    );
    assert_eq!(body[1]["start_request_id"], r1);
    Ok(())
}

async fn assert_session_turns_asc_and_invalid_order(
    client: &reqwest::Client,
    url: &str,
    r1: i64,
    r2: i64,
) -> anyhow::Result<()> {
    let resp = client.get(format!("{url}?order=asc")).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let asc_body: Vec<serde_json::Value> = resp.json().await?;
    assert_eq!(asc_body.len(), 2);
    assert_eq!(
        asc_body[0]["start_request_id"], r1,
        "order=asc must be chronological (oldest turn first)"
    );
    assert_eq!(asc_body[1]["start_request_id"], r2);

    let resp = client.get(format!("{url}?order=latest")).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    Ok(())
}

async fn assert_outcome_and_by_outcome_filters(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let url = format!(
        "{}/api/operator-turns?outcome=corrected&limit=10",
        inspect_url
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json().await?;
    assert!(
        body.iter()
            .any(|t| t["session_id"] == session_id && t["outcome"] == "corrected"),
        "list must include our corrected turn"
    );

    let url = format!(
        "{}/api/operator-turns/by-outcome?outcome=corrected",
        inspect_url
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json().await?;
    assert!(
        body.iter().any(|t| t["session_id"] == session_id),
        "by-outcome=corrected must include our session"
    );
    Ok(())
}

async fn assert_by_outcome_missing_and_summary_endpoint(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let url = format!("{}/api/operator-turns/by-outcome", inspect_url);
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    let url = format!("{}/api/operator-turns/outcome-summary", inspect_url);
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let summary: serde_json::Value = resp.json().await?;
    assert!(summary["total"].as_i64().unwrap_or(0) >= 2);
    let counts = summary["counts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("counts array missing in {summary}"))?;
    assert!(
        counts
            .iter()
            .any(|c| c["outcome"] == "corrected" && c["count"].as_i64().unwrap_or(0) >= 1),
        "summary must show corrected count: {summary}"
    );
    Ok(())
}

async fn assert_single_turn_endpoint(
    client: &reqwest::Client,
    inspect_url: &str,
    turn1_id: i64,
    long_prompt: &str,
) -> anyhow::Result<()> {
    let url = format!("{}/api/operator-turns/{}", inspect_url, turn1_id);
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let row: serde_json::Value = resp.json().await?;
    assert_eq!(row["turn_id"], turn1_id);
    assert_eq!(row["user_prompt"], long_prompt);

    let url = format!("{}/api/operator-turns/9999999999", inspect_url);
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn operator_turns_rest_surface_returns_full_rows_with_no_truncation() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-operator-turns-rest";
    let session_id = "12345678-1234-4321-8765-123456789abc";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    let long_prompt = "do thing A: ".to_string() + &"x".repeat(2_000);
    let (r1, r2, turn1_id) = seed_two_turns(&pool, project_id, session_id, &long_prompt).await?;

    let client = reqwest::Client::new();
    let turns_url = format!("{inspect_url}/api/sessions/{session_id}/operator-turns");

    assert_session_turns_default_order(&client, &turns_url, r1, r2, &long_prompt).await?;
    assert_session_turns_asc_and_invalid_order(&client, &turns_url, r1, r2).await?;
    assert_outcome_and_by_outcome_filters(&client, &inspect_url, session_id).await?;
    assert_by_outcome_missing_and_summary_endpoint(&client, &inspect_url).await?;
    assert_single_turn_endpoint(&client, &inspect_url, turn1_id, &long_prompt).await?;

    test_db.cleanup().await?;
    Ok(())
}
