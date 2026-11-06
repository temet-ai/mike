use crate::db_helpers;

use serde_json::{Value, json};

use super::support::{InsertOperatorTurnParams, bootstrap_project_session, insert_operator_turn};

#[tokio::test]
async fn recall_project_context_bundles_corrected_shipped_and_lessons() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-recall-context";
    let session_id = "12345678-cccc-4ccc-8ccc-cccccccccccc";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    let long_corrected_prompt = "do thing A: ".to_string() + &"y".repeat(2_000);
    let _corrected = insert_operator_turn(
        &pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: 0,
            end_request_id: 0,
            user_prompt: &long_corrected_prompt,
            final_assistant_text: "starting",
            files_touched: &json!([]),
            outcome: "corrected",
            outcome_evidence: Some("no, redo it"),
        },
    )
    .await?;
    let _shipped = insert_operator_turn(
        &pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: 0,
            end_request_id: 0,
            user_prompt: "ship feature X",
            final_assistant_text: "shipped result",
            files_touched: &json!([]),
            outcome: "shipped",
            outcome_evidence: None,
        },
    )
    .await?;

    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text)
         VALUES ($1, (SELECT id FROM memory.lesson_kinds WHERE code = 'preference'),
                 'always include integration tests')",
    )
    .bind(project_id)
    .execute(&pool)
    .await?;

    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/recall/project-context?project={}",
        inspect_url, project_path
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    assert_eq!(body["project_path"], project_path);
    let corrected = body["recent_corrected"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recent_corrected must be array: {body}"))?;
    assert!(
        corrected
            .iter()
            .any(|t| t["outcome"] == "corrected" && t["session_id"] == session_id),
        "corrected list must include our turn: {body}"
    );
    let shipped = body["recent_shipped"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recent_shipped must be array: {body}"))?;
    assert!(
        shipped
            .iter()
            .any(|t| t["outcome"] == "shipped" && t["session_id"] == session_id),
        "shipped list must include our turn: {body}"
    );
    let lessons = body["lessons"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("lessons must be array: {body}"))?;
    assert!(
        lessons
            .iter()
            .any(|l| l["kind"] == "preference" && l["text"] == "always include integration tests"),
        "lessons must include our seeded preference: {body}"
    );
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_project_context_missing_project_returns_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let url = format!("{}/api/recall/project-context", inspect_url);
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    test_db.cleanup().await?;
    Ok(())
}
