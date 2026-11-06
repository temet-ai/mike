use crate::db_helpers;

use super::support::{register_with_pid, start_proxy_with_shared_db_and_state};
use serde_json::{Value, json};

fn rand_test_pid() -> i32 {
    (uuid::Uuid::new_v4().as_u128() % 1_000_000_000) as i32 + 1_000_000
}

#[tokio::test]
async fn wake_pending_unknown_pid_returns_404() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let unknown_pid = rand_test_pid();
    let resp = client
        .get(format!(
            "{}/api/workspace/wake-pending?pid={unknown_pid}",
            handle.inspect_url
        ))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], json!("registration_not_found"), "{body}");

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn wake_pending_known_pid_with_queued_reminder_returns_counts() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let project_root = format!("/tmp/workspace-e2e-{}", uuid::Uuid::new_v4());
    let session = uuid::Uuid::new_v4();
    let pid = rand_test_pid();

    let (status, body) =
        register_with_pid(&client, &handle.inspect_url, session, &project_root, pid).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    let send_resp = client
        .post(format!(
            "{}/api/sessions/{session}/message",
            handle.inspect_url
        ))
        .json(&json!({"message": "wake up"}))
        .send()
        .await?;
    assert_eq!(
        send_resp.status(),
        reqwest::StatusCode::CREATED,
        "{}",
        send_resp.text().await.unwrap_or_default()
    );

    let resp = client
        .get(format!(
            "{}/api/workspace/wake-pending?pid={pid}",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    assert_eq!(body["session_id"], json!(session.to_string()), "{body}");
    assert_eq!(body["pending_total"], json!(1), "{body}");
    assert_eq!(body["pending_steering"], json!(0), "{body}");
    assert_eq!(body["busy"], json!(false), "{body}");

    let resp2 = client
        .get(format!(
            "{}/api/workspace/wake-pending?pid={pid}",
            handle.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp2.status(), reqwest::StatusCode::OK);
    let body2: Value = resp2.json().await?;
    assert_eq!(body2["pending_total"], json!(1), "{body2}");
    assert_eq!(body2["pending_steering"], json!(0), "{body2}");

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn wake_pending_busy_session_reflects_in_flight() -> anyhow::Result<()> {
    let Some((inspect_url, state, shutdown, test_db)) =
        start_proxy_with_shared_db_and_state("http://127.0.0.1:1").await?
    else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let project_root = format!("/tmp/workspace-e2e-{}", uuid::Uuid::new_v4());
    let session = uuid::Uuid::new_v4();
    let pid = rand_test_pid();

    let (status, body) = register_with_pid(&client, &inspect_url, session, &project_root, pid)
        .await
        .map_err(|e| anyhow::anyhow!("register_with_pid: {e}"))?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    let _synthetic_id = mike::in_flight::register_for_tests(&state, session.to_string());

    let resp = client
        .get(format!(
            "{inspect_url}/api/workspace/wake-pending?pid={pid}"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    assert_eq!(body["session_id"], json!(session.to_string()), "{body}");
    assert_eq!(body["busy"], json!(true), "{body}");

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn wake_pending_survives_lease_lapse_for_alive_pid() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let project_root = format!("/tmp/workspace-e2e-{}", uuid::Uuid::new_v4());
    let session = uuid::Uuid::new_v4();
    let pid = rand_test_pid();

    let (status, body) =
        register_with_pid(&client, &handle.inspect_url, session, &project_root, pid).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    sqlx::query(
        "UPDATE workspace.session_registrations
            SET lease_expires_at = now() - interval '20 minutes'
          WHERE pid = $1",
    )
    .bind(pid)
    .execute(&handle.pool)
    .await?;

    let resp = client
        .get(format!(
            "{}/api/workspace/wake-pending?pid={pid}",
            handle.inspect_url
        ))
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "a lease-lapsed-but-alive registration must still resolve — issue #121: {body}"
    );
    assert_eq!(body["session_id"], json!(session.to_string()), "{body}");

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
