use crate::db_helpers;

use serde_json::{Value, json};

use super::support::{InsertOperatorTurnParams, bootstrap_project_session, insert_operator_turn};

#[tokio::test]
async fn recall_file_history_matches_files_touched_jsonb_contains() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-file-history";
    let session_id = "12345678-dddd-4ddd-8ddd-dddddddddddd";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    let _hit = insert_operator_turn(
        &pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: 0,
            end_request_id: 0,
            user_prompt: "edit /target.rs",
            final_assistant_text: "done",
            files_touched: &json!([{"path":"/target.rs","edits":3,"reads":1,"other":0}]),
            outcome: "shipped",
            outcome_evidence: None,
        },
    )
    .await?;
    let _miss = insert_operator_turn(
        &pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: 0,
            end_request_id: 0,
            user_prompt: "unrelated",
            final_assistant_text: "done",
            files_touched: &json!([{"path":"/elsewhere.rs","edits":1,"reads":0,"other":0}]),
            outcome: "shipped",
            outcome_evidence: None,
        },
    )
    .await?;

    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/recall/file-history?project={}&file=/target.rs&limit=10",
        inspect_url, project_path
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Vec<Value> = resp.json().await?;
    assert_eq!(body.len(), 1, "expected exactly one hit, got {body:?}");
    assert_eq!(body[0]["user_prompt"], "edit /target.rs");
    test_db.cleanup().await?;
    Ok(())
}
