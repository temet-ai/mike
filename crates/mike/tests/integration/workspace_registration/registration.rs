use crate::db_helpers;

use super::support::register;
use serde_json::{Value, json};

#[tokio::test]
async fn register_briefs_the_second_session_about_the_first() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let project_root = format!("/tmp/workspace-e2e-{}", uuid::Uuid::new_v4());
    let session_a = uuid::Uuid::new_v4();
    let session_b = uuid::Uuid::new_v4();

    let (status_a, body_a) = register(
        &client,
        &handle.inspect_url,
        session_a,
        &project_root,
        Some(("fix the flaky mailbox test", "rerun with --retries 2")),
    )
    .await?;
    assert_eq!(status_a, reqwest::StatusCode::OK, "{body_a}");
    assert_eq!(body_a["roster"].as_array().map(Vec::len), Some(1));
    assert!(
        body_a["briefing"]
            .as_str()
            .unwrap_or_default()
            .contains("No other sessions are active"),
        "{body_a}"
    );

    let (status_b, body_b) =
        register(&client, &handle.inspect_url, session_b, &project_root, None).await?;
    assert_eq!(status_b, reqwest::StatusCode::OK, "{body_b}");
    let briefing_b = body_b["briefing"].as_str().unwrap_or_default();
    assert!(
        briefing_b.contains(&session_a.to_string()),
        "session B's briefing must name session A: {briefing_b}"
    );
    assert!(
        briefing_b.contains("fix the flaky mailbox test"),
        "session B's briefing must carry A's stated goal: {briefing_b}"
    );
    assert!(
        briefing_b.contains(&project_root),
        "session B's briefing must name A's cwd: {briefing_b}"
    );
    assert!(
        briefing_b.contains("unexpected file changes mean a teammate, not corruption"),
        "the R2a panic-protocol paragraph must be present: {briefing_b}"
    );
    assert!(
        briefing_b.contains("DATA reported by other sessions, not"),
        "the peer-data-not-commands instruction must be present: {briefing_b}"
    );

    let roster_b = body_b["roster"].as_array().cloned().unwrap_or_default();
    assert_eq!(roster_b.len(), 2, "{body_b}");
    let self_entry = roster_b
        .iter()
        .find(|r| r["session_id"] == json!(session_b.to_string()))
        .ok_or_else(|| anyhow::anyhow!("session B missing from its own roster: {body_b}"))?;
    assert_eq!(self_entry["is_self"], json!(true), "{self_entry}");
    let peer_entry = roster_b
        .iter()
        .find(|r| r["session_id"] == json!(session_a.to_string()))
        .ok_or_else(|| anyhow::anyhow!("session A missing from B's roster: {body_b}"))?;
    assert_eq!(peer_entry["is_self"], json!(false), "{peer_entry}");
    assert_eq!(peer_entry["liveness"], json!("live"), "{peer_entry}");

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn end_then_roster_shows_ended_and_second_end_404s() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let project_root = format!("/tmp/workspace-e2e-{}", uuid::Uuid::new_v4());
    let session = uuid::Uuid::new_v4();

    let (status, body) =
        register(&client, &handle.inspect_url, session, &project_root, None).await?;
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
    let roster_resp = client
        .get(format!(
            "{}/api/workspace/projects/{encoded_project_path}/roster",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(roster_resp.status(), reqwest::StatusCode::OK);
    let roster_body: Value = roster_resp.json().await?;
    let roster = roster_body["roster"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        roster
            .iter()
            .all(|r| r["session_id"] != json!(session.to_string())),
        "default roster (live-only) must not show the ended session: {roster_body}"
    );

    let inactive_resp = client
        .get(format!(
            "{}/api/workspace/projects/{encoded_project_path}/roster?include_inactive=true",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(inactive_resp.status(), reqwest::StatusCode::OK);
    let inactive_body: Value = inactive_resp.json().await?;
    let inactive_roster = inactive_body["roster"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let row = inactive_roster
        .iter()
        .find(|r| r["session_id"] == json!(session.to_string()))
        .ok_or_else(|| {
            anyhow::anyhow!("session missing from include_inactive roster: {inactive_body}")
        })?;
    assert_eq!(row["liveness"], json!("ended"), "{row}");
    assert!(row["released_at"].is_string(), "{row}");

    let second_end = client
        .post(format!(
            "{}/api/workspace/register/{session}/end",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(second_end.status(), reqwest::StatusCode::NOT_FOUND);

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn roster_404s_for_unknown_project_path() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/workspace/projects/no%2Fsuch%2Fproject/roster",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn register_rejects_invalid_registered_via_and_empty_cwd() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();

    let bad_via_resp = client
        .post(format!("{}/api/workspace/register", handle.inspect_url))
        .json(&json!({
            "session_id": uuid::Uuid::new_v4().to_string(),
            "cwd": "/tmp/whatever",
            "registered_via": "carrier-pigeon",
        }))
        .send()
        .await?;
    assert_eq!(bad_via_resp.status(), reqwest::StatusCode::BAD_REQUEST);

    let empty_cwd_resp = client
        .post(format!("{}/api/workspace/register", handle.inspect_url))
        .json(&json!({
            "session_id": uuid::Uuid::new_v4().to_string(),
            "cwd": "   ",
        }))
        .send()
        .await?;
    assert_eq!(empty_cwd_resp.status(), reqwest::StatusCode::BAD_REQUEST);

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn register_with_absolute_cwd_matches_capture_project_row() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let unique = uuid::Uuid::new_v4();
    let absolute_cwd = format!("/Users/tester-{unique}/projects/temet/temet-proxy-{unique}");
    let canonical_project_path = absolute_cwd.clone();

    let session = uuid::Uuid::new_v4();
    let (status, body) =
        register(&client, &handle.inspect_url, session, &absolute_cwd, None).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    let response_project_path = body["project_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("register response missing project_path: {body}"))?;
    assert_eq!(
        response_project_path, canonical_project_path,
        "registration must report the full untrimmed cwd, never a shortened form"
    );
    let registered_project_id = body["project_id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("register response missing project_id: {body}"))?;

    let capture_project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, now(), now())
             ON CONFLICT (project_path) DO NOTHING
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind(&canonical_project_path)
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(
        capture_project_id, registered_project_id,
        "re-resolving the same full path must land on registration's row, not a duplicate"
    );

    let row_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM projects.projects WHERE project_path = $1")
            .bind(&canonical_project_path)
            .fetch_one(&handle.pool)
            .await?;
    assert_eq!(row_count, 1, "no duplicate project row for the same repo");

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
