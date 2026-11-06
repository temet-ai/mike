use crate::db_helpers;

use serde_json::{Value, json};

#[tokio::test]
async fn lesson_topic_survives_the_write_and_filters_the_read() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();

    let project_path = "test-lesson-topic";
    let client = reqwest::Client::new();

    let with_topic = client
        .post(format!("{}/api/lessons", inspect_url))
        .json(&json!({
            "project_path": project_path,
            "kind": "warning",
            "text": "Left unmet when session probe ended stopped-unverified: the panel is reachable",
            "topic": "verifier:unmet",
        }))
        .send()
        .await?;
    assert_eq!(with_topic.status(), reqwest::StatusCode::CREATED);
    let created: Value = with_topic.json().await?;
    let topical_id = created["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id must be an integer: {created}"))?;
    assert_eq!(
        created["topic"], "verifier:unmet",
        "the created row must echo the topic it was given: {created}"
    );

    let plain = client
        .post(format!("{}/api/lessons", inspect_url))
        .json(&json!({
            "project_path": project_path,
            "kind": "convention",
            "text": "An ordinary lesson carrying no topic.",
        }))
        .send()
        .await?;
    assert_eq!(plain.status(), reqwest::StatusCode::CREATED);
    let plain_id = plain.json::<Value>().await?["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id must be an integer"))?;

    let all: Vec<Value> = client
        .get(format!(
            "{}/api/lessons?project={}",
            inspect_url, project_path
        ))
        .send()
        .await?
        .json()
        .await?;
    let stored = all
        .iter()
        .find(|l| l["id"] == topical_id)
        .ok_or_else(|| anyhow::anyhow!("the topical lesson is missing from the list"))?;
    assert_eq!(
        stored["topic"], "verifier:unmet",
        "the topic must be stored, not just echoed back by the create response: {stored}"
    );

    let filtered: Vec<Value> = client
        .get(format!(
            "{}/api/lessons?project={}&topic=verifier:unmet",
            inspect_url, project_path
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        filtered.len(),
        1,
        "expected exactly the topical row: {filtered:?}"
    );
    assert_eq!(filtered[0]["id"], topical_id);

    let empty: Vec<Value> = client
        .get(format!(
            "{}/api/lessons?project={}&topic=verifier:met",
            inspect_url, project_path
        ))
        .send()
        .await?
        .json()
        .await?;
    assert!(
        empty.is_empty(),
        "an unused topic key must match nothing, not fall back to the full list: {empty:?}"
    );
    assert_ne!(plain_id, topical_id);

    test_db.cleanup().await?;
    Ok(())
}
