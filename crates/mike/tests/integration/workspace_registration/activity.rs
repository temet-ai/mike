use crate::db_helpers;

use super::support::register;
use serde_json::{Value, json};

fn assert_registered_event_is_full_fidelity(registered_event: &Value, session: uuid::Uuid) {
    assert_eq!(
        registered_event["session_id"],
        json!(session.to_string()),
        "{registered_event}"
    );
    assert_eq!(
        registered_event["subject"],
        json!(session.to_string()),
        "{registered_event}"
    );
    assert!(
        registered_event["actor_label"]
            .as_str()
            .unwrap_or_default()
            .contains(&session.to_string()),
        "{registered_event}"
    );
    assert!(registered_event["detail"].is_object(), "{registered_event}");
    assert!(
        chrono::DateTime::parse_from_rfc3339(
            registered_event["created_at"].as_str().unwrap_or_default()
        )
        .is_ok(),
        "created_at must be rfc3339: {registered_event}"
    );
}

#[tokio::test]
async fn activity_returns_ledger_rows_full_fidelity_and_paginated() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let project_root = format!("/tmp/workspace-e2e-{}", uuid::Uuid::new_v4());
    let session = uuid::Uuid::new_v4();

    let (status, body) = register(
        &client,
        &handle.inspect_url,
        session,
        &project_root,
        Some(("ship the activity feed", "wire the route then test it")),
    )
    .await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let project_path = body["project_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("register response missing project_path: {body}"))?
        .to_string();

    let end_resp = client
        .post(format!(
            "{}/api/workspace/register/{session}/end",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(end_resp.status(), reqwest::StatusCode::NO_CONTENT);

    let encoded_project_path = project_path.replace('/', "%2F");

    let activity_resp = client
        .get(format!(
            "{}/api/workspace/projects/{encoded_project_path}/activity",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(activity_resp.status(), reqwest::StatusCode::OK);
    let activity_body: Value = activity_resp.json().await?;
    assert_eq!(activity_body["project_path"], json!(project_path));
    assert_eq!(activity_body["total_count"], json!(2), "{activity_body}");
    let events = activity_body["events"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(events.len(), 2, "{activity_body}");
    assert_eq!(events[0]["kind"], json!("ended"), "{events:?}");
    assert_eq!(events[1]["kind"], json!("registered"), "{events:?}");
    assert_registered_event_is_full_fidelity(&events[1], session);

    let page_resp = client
        .get(format!(
            "{}/api/workspace/projects/{encoded_project_path}/activity?limit=1&offset=0",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(page_resp.status(), reqwest::StatusCode::OK);
    let page_body: Value = page_resp.json().await?;
    assert_eq!(page_body["total_count"], json!(2), "{page_body}");
    assert_eq!(page_body["limit"], json!(1), "{page_body}");
    assert_eq!(page_body["offset"], json!(0), "{page_body}");
    let page_events = page_body["events"].as_array().cloned().unwrap_or_default();
    assert_eq!(page_events.len(), 1, "{page_body}");
    assert_eq!(page_events[0]["kind"], json!("ended"), "{page_body}");

    let second_page_resp = client
        .get(format!(
            "{}/api/workspace/projects/{encoded_project_path}/activity?limit=1&offset=1",
            handle.inspect_url
        ))
        .send()
        .await?;
    let second_page_body: Value = second_page_resp.json().await?;
    let second_page_events = second_page_body["events"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(second_page_events.len(), 1, "{second_page_body}");
    assert_eq!(
        second_page_events[0]["kind"],
        json!("registered"),
        "{second_page_body}"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn activity_404s_for_unknown_project_path() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/workspace/projects/no%2Fsuch%2Fproject/activity",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
