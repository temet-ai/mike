use crate::db_helpers;

use super::support::register;
use serde_json::{Value, json};

#[tokio::test]
async fn roster_resolves_unique_project_path_suffix() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let unique = uuid::Uuid::new_v4();
    let repo_name = format!("temet-proxy-{unique}");
    let absolute_cwd = format!("/Users/tester-{unique}/projects/temet/{repo_name}");
    let canonical_project_path = absolute_cwd.clone();

    let session = uuid::Uuid::new_v4();
    let (status, body) =
        register(&client, &handle.inspect_url, session, &absolute_cwd, None).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let registered_project_id = body["project_id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("register response missing project_id: {body}"))?;

    let resp = client
        .get(format!(
            "{}/api/workspace/projects/{repo_name}/roster",
            handle.inspect_url
        ))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(body["project_id"], json!(registered_project_id), "{body}");
    assert_eq!(
        body["project_path"],
        json!(canonical_project_path),
        "response must echo the real stored path, not the suffix the caller sent: {body}"
    );
    let roster = body["roster"].as_array().cloned().unwrap_or_default();
    assert!(
        roster
            .iter()
            .any(|r| r["session_id"] == json!(session.to_string())),
        "session missing from the suffix-resolved roster: {body}"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn roster_ambiguous_project_path_suffix_returns_409_with_candidates() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let unique = uuid::Uuid::new_v4();
    let repo_name = format!("temet-proxy-{unique}");
    let cwd_a = format!("/Users/tester-{unique}/projects/org-a/{repo_name}");
    let cwd_b = format!("/Users/tester-{unique}/projects/org-b/{repo_name}");
    let canonical_a = format!("projects/org-a/{repo_name}");
    let canonical_b = format!("projects/org-b/{repo_name}");

    let (status_a, body_a) = register(
        &client,
        &handle.inspect_url,
        uuid::Uuid::new_v4(),
        &cwd_a,
        None,
    )
    .await?;
    assert_eq!(status_a, reqwest::StatusCode::OK, "{body_a}");
    let (status_b, body_b) = register(
        &client,
        &handle.inspect_url,
        uuid::Uuid::new_v4(),
        &cwd_b,
        None,
    )
    .await?;
    assert_eq!(status_b, reqwest::StatusCode::OK, "{body_b}");

    let resp = client
        .get(format!(
            "{}/api/workspace/projects/{repo_name}/roster",
            handle.inspect_url
        ))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "ambiguous_project_path", "{body}");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(&canonical_a) && message.contains(&canonical_b),
        "ambiguous response must list every candidate path: {message}"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn roster_zero_match_project_path_suffix_returns_404_with_known_paths() -> anyhow::Result<()>
{
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let unique = uuid::Uuid::new_v4();
    let known_cwd = format!("/Users/tester-{unique}/projects/temet/known-project-{unique}");
    let known_canonical = format!("projects/temet/known-project-{unique}");
    let (status, body) = register(
        &client,
        &handle.inspect_url,
        uuid::Uuid::new_v4(),
        &known_cwd,
        None,
    )
    .await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    let resp = client
        .get(format!(
            "{}/api/workspace/projects/no-such-project-{unique}/roster",
            handle.inspect_url
        ))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "project_not_found", "{body}");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(&known_canonical),
        "the 404 must name a known project path as a candidate: {message}"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
