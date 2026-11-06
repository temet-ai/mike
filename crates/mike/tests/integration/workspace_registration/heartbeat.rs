use crate::db_helpers;

use super::support::register;
use serde_json::{Value, json};

#[tokio::test]
async fn heartbeat_renews_lease_and_reports_inbox_counts() -> anyhow::Result<()> {
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
    let lease_before = body["lease_expires_at"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("register response missing lease_expires_at: {body}"))?
        .to_string();

    let to_session_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(&handle.pool)
            .await?;

    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, body, claimed_at, sent_at)
         VALUES ($1, $2, 'unconsumed message', now(), now())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(to_session_id)
    .execute(&handle.pool)
    .await?;
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, body)
         VALUES ($1, $2, 'never claimed message')",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(to_session_id)
    .execute(&handle.pool)
    .await?;

    let heartbeat_resp = client
        .post(format!("{}/api/workspace/heartbeat", handle.inspect_url))
        .json(&json!({"session_id": session.to_string()}))
        .send()
        .await?;
    assert_eq!(heartbeat_resp.status(), reqwest::StatusCode::OK);
    let heartbeat_body: Value = heartbeat_resp.json().await?;
    assert_eq!(
        heartbeat_body["inbox"]["unconsumed"],
        json!(1),
        "{heartbeat_body}"
    );
    assert_eq!(
        heartbeat_body["inbox"]["unclaimed"],
        json!(1),
        "{heartbeat_body}"
    );
    assert_eq!(
        heartbeat_body["inbox"]["dead_lettered_outbox"],
        json!(0),
        "no dead-lettered sends yet: {heartbeat_body}"
    );
    let lease_after = heartbeat_body["lease_expires_at"].as_str().ok_or_else(|| {
        anyhow::anyhow!("heartbeat response missing lease_expires_at: {heartbeat_body}")
    })?;
    assert!(
        lease_after >= lease_before.as_str(),
        "heartbeat must not shorten the lease: before={lease_before} after={lease_after}"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn heartbeat_reports_dead_lettered_outbox_count() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();
    let sender = uuid::Uuid::new_v4();
    let recipient = uuid::Uuid::new_v4();
    let sender_root = format!("/tmp/workspace-e2e-sender-{}", uuid::Uuid::new_v4());
    let recipient_root = format!("/tmp/workspace-e2e-recipient-{}", uuid::Uuid::new_v4());

    let (status, body) = register(&client, &handle.inspect_url, sender, &sender_root, None).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let (status, body) = register(
        &client,
        &handle.inspect_url,
        recipient,
        &recipient_root,
        None,
    )
    .await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    let sender_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(sender)
            .fetch_one(&handle.pool)
            .await?;
    let recipient_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(recipient)
            .fetch_one(&handle.pool)
            .await?;

    sqlx::query(
        "INSERT INTO mailbox.messages
             (message_uuid, to_session_id, from_session_id, body, claimed_at,
              sent_at, sent_request_id, delivery_attempts, dead_lettered_at)
         VALUES ($1, $2, $3, 'never delivered in time', now(), now(), NULL, 5, now())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(recipient_id)
    .bind(sender_id)
    .execute(&handle.pool)
    .await?;

    let heartbeat_resp = client
        .post(format!("{}/api/workspace/heartbeat", handle.inspect_url))
        .json(&json!({"session_id": sender.to_string()}))
        .send()
        .await?;
    assert_eq!(heartbeat_resp.status(), reqwest::StatusCode::OK);
    let heartbeat_body: Value = heartbeat_resp.json().await?;
    assert_eq!(
        heartbeat_body["inbox"]["dead_lettered_outbox"],
        json!(1),
        "the sender's own heartbeat must surface its dead-lettered send: {heartbeat_body}"
    );
    assert_eq!(
        heartbeat_body["inbox"]["unconsumed"],
        json!(0),
        "{heartbeat_body}"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn heartbeat_404s_without_active_registration() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping workspace_registration test");
        return Ok(());
    };

    let client = reqwest::Client::new();

    let never_registered = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{}/api/workspace/heartbeat", handle.inspect_url))
        .json(&json!({"session_id": never_registered.to_string()}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    let project_root = format!("/tmp/workspace-e2e-{}", uuid::Uuid::new_v4());
    let session = uuid::Uuid::new_v4();
    let (status, _body) =
        register(&client, &handle.inspect_url, session, &project_root, None).await?;
    assert_eq!(status, reqwest::StatusCode::OK);
    client
        .post(format!(
            "{}/api/workspace/register/{session}/end",
            handle.inspect_url
        ))
        .send()
        .await?
        .error_for_status()?;

    let resp = client
        .post(format!("{}/api/workspace/heartbeat", handle.inspect_url))
        .json(&json!({"session_id": session.to_string()}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
