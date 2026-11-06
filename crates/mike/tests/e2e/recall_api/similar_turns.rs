use crate::db_helpers;

use serde_json::{Value, json};

use super::support::{InsertOperatorTurnParams, bootstrap_project_session, insert_operator_turn};

#[tokio::test]
async fn recall_similar_turns_fts_ranks_known_phrase_higher() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-fts";
    let session_id = "12345678-eeee-4eee-8eee-eeeeeeeeeeee";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    let _t1 = insert_operator_turn(
        &pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: 0,
            end_request_id: 0,
            user_prompt: "implement quicksort algorithm in Rust",
            final_assistant_text: "added quicksort to sorting module",
            files_touched: &json!([]),
            outcome: "shipped",
            outcome_evidence: None,
        },
    )
    .await?;
    let _t2 = insert_operator_turn(
        &pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: 0,
            end_request_id: 0,
            user_prompt: "configure docker compose volumes",
            final_assistant_text: "updated compose.yaml volumes",
            files_touched: &json!([]),
            outcome: "shipped",
            outcome_evidence: None,
        },
    )
    .await?;
    let _t3 = insert_operator_turn(
        &pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: 0,
            end_request_id: 0,
            user_prompt: "rename quicksort variable",
            final_assistant_text: "renamed quicksort to sort_quick",
            files_touched: &json!([]),
            outcome: "shipped",
            outcome_evidence: None,
        },
    )
    .await?;

    let client = reqwest::Client::new();
    let url = format!("{}/api/recall/similar-turns", inspect_url);
    let resp = client
        .post(&url)
        .json(&json!({
            "project": project_path,
            "query": "quicksort algorithm",
            "limit": 5,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    let rows = body["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("turns must be an array: {body}"))?;
    assert!(!rows.is_empty(), "FTS must return at least one match");
    let first = &rows[0];
    let user_prompt = first["user_prompt"].as_str().unwrap_or("");
    assert!(
        user_prompt.contains("quicksort"),
        "top FTS match must contain 'quicksort': {first}"
    );
    assert!(
        !rows.iter().any(|r| r["user_prompt"]
            .as_str()
            .unwrap_or("")
            .contains("docker compose")),
        "docker compose row must not match 'quicksort algorithm': {rows:?}"
    );
    assert!(
        first["rank"].as_f64().unwrap_or(0.0) > 0.0,
        "FTS rank must be positive: {first}"
    );
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_similar_turns_missing_query_returns_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let url = format!("{}/api/recall/similar-turns", inspect_url);
    let resp = client
        .post(&url)
        .json(&json!({"project": "x", "query": ""}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    test_db.cleanup().await?;
    Ok(())
}
