use crate::db_helpers;

use serde_json::{Value, json};

#[tokio::test]
async fn lessons_crud_round_trip_preserves_full_text() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();

    let project_path = "test-lessons-crud";
    let client = reqwest::Client::new();

    let long_text = format!(
        "When refactoring the auth module, always run the integration suite: {}",
        "z".repeat(3_000)
    );

    let create_resp = client
        .post(format!("{}/api/lessons", inspect_url))
        .json(&json!({
            "project_path": project_path,
            "kind": "convention",
            "text": long_text,
        }))
        .send()
        .await?;
    assert_eq!(create_resp.status(), reqwest::StatusCode::CREATED);
    let created: Value = create_resp.json().await?;
    let id = created["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id must be an integer: {created}"))?;
    assert_eq!(created["text"], long_text);
    assert_eq!(created["kind"], "convention");

    let list_resp = client
        .get(format!(
            "{}/api/lessons?project={}",
            inspect_url, project_path
        ))
        .send()
        .await?;
    assert_eq!(list_resp.status(), reqwest::StatusCode::OK);
    let lessons: Vec<Value> = list_resp.json().await?;
    assert!(
        lessons
            .iter()
            .any(|l| l["id"] == id && l["text"] == long_text),
        "list must include our lesson with full text: {lessons:?}"
    );

    let del_resp = client
        .delete(format!("{}/api/lessons/{}", inspect_url, id))
        .send()
        .await?;
    assert_eq!(del_resp.status(), reqwest::StatusCode::NO_CONTENT);

    let after_resp = client
        .get(format!(
            "{}/api/lessons?project={}",
            inspect_url, project_path
        ))
        .send()
        .await?;
    let after: Vec<Value> = after_resp.json().await?;
    assert!(
        !after.iter().any(|l| l["id"] == id),
        "deleted lesson must not reappear: {after:?}"
    );

    let del_again = client
        .delete(format!("{}/api/lessons/{}", inspect_url, id))
        .send()
        .await?;
    assert_eq!(del_again.status(), reqwest::StatusCode::NOT_FOUND);
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn lessons_create_rejects_invalid_kind() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/lessons", inspect_url))
        .json(&json!({
            "project_path": "anywhere",
            "kind": "not-a-valid-kind",
            "text": "x",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    test_db.cleanup().await?;
    Ok(())
}
