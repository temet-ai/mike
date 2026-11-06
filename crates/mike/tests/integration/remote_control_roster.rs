use crate::db_helpers;

use serde_json::{Value, json};

async fn register(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: uuid::Uuid,
    cwd: &str,
) -> anyhow::Result<(reqwest::StatusCode, Value)> {
    let body = json!({
        "session_id": session_id.to_string(),
        "cwd": cwd,
        "repo_root": cwd,
    });
    let resp = client
        .post(format!("{inspect_url}/api/workspace/register"))
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let json: Value = resp.json().await?;
    Ok((status, json))
}

async fn insert_channel(
    pool: &sqlx::PgPool,
    cse_id: &str,
    session_uuid: uuid::Uuid,
    age_seconds: i64,
    worker_status: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO remote_control.channels
             (cse_id, local_session_uuid, first_seen_at, last_seen_at,
              last_heartbeat_at, last_heartbeat_interval_seconds,
              worker_status, worker_status_at)
         VALUES ($1, $2, now(), now(), now() - ($3 || ' seconds')::interval, 20, $4, now())",
    )
    .bind(cse_id)
    .bind(session_uuid)
    .bind(age_seconds.to_string())
    .bind(worker_status)
    .execute(pool)
    .await?;
    Ok(())
}

fn find_entry(roster: &[Value], session_id: uuid::Uuid) -> &Value {
    roster
        .iter()
        .find(|r| r["session_id"] == json!(session_id.to_string()))
        .unwrap_or_else(|| panic!("session {session_id} missing from roster: {roster:?}"))
}

#[tokio::test]
async fn a_recently_heartbeating_channel_reads_live_with_channel_source() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping remote_control_roster test");
        return Ok(());
    };
    let client = reqwest::Client::new();
    let project_root = format!("/tmp/rc-roster-live-{}", uuid::Uuid::new_v4());
    let with_channel = uuid::Uuid::new_v4();
    let without_channel = uuid::Uuid::new_v4();

    let (status, body) =
        register(&client, &handle.inspect_url, with_channel, &project_root).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    insert_channel(
        &handle.pool,
        &format!("cse_test_{with_channel}"),
        with_channel,
        0,
        "idle",
    )
    .await?;

    let (status, body) =
        register(&client, &handle.inspect_url, without_channel, &project_root).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let roster = body["roster"].as_array().cloned().unwrap_or_default();
    assert_eq!(roster.len(), 2, "{body}");

    let channelled = find_entry(&roster, with_channel);
    assert_eq!(channelled["channel_present"], json!(true), "{channelled}");
    assert_eq!(
        channelled["liveness_source"],
        json!("channel"),
        "{channelled}"
    );
    assert_eq!(channelled["liveness"], json!("live"), "{channelled}");
    assert_eq!(channelled["session_state"], json!("idle"), "{channelled}");
    assert!(
        channelled["heartbeat_age_seconds"]
            .as_i64()
            .unwrap_or(i64::MAX)
            < 10,
        "{channelled}"
    );

    let channelless = find_entry(&roster, without_channel);
    assert_eq!(
        channelless["channel_present"],
        json!(false),
        "{channelless}"
    );
    assert_eq!(
        channelless["liveness_source"],
        json!("lease"),
        "{channelless}"
    );
    assert_eq!(channelless["liveness"], json!("live"), "{channelless}");

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_channel_overdue_by_more_than_three_intervals_reads_stale() -> anyhow::Result<()> {
    let Some(handle) = db_helpers::start_proxy_with_shared_db("http://127.0.0.1:1").await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping remote_control_roster test");
        return Ok(());
    };
    let client = reqwest::Client::new();
    let project_root = format!("/tmp/rc-roster-stale-{}", uuid::Uuid::new_v4());
    let stale_channel = uuid::Uuid::new_v4();
    let anchor = uuid::Uuid::new_v4();

    let (status, body) =
        register(&client, &handle.inspect_url, stale_channel, &project_root).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    insert_channel(
        &handle.pool,
        &format!("cse_test_{stale_channel}"),
        stale_channel,
        300,
        "running",
    )
    .await?;

    let (status, body) = register(&client, &handle.inspect_url, anchor, &project_root).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let roster = body["roster"].as_array().cloned().unwrap_or_default();

    let entry = find_entry(&roster, stale_channel);
    assert_eq!(entry["channel_present"], json!(true), "{entry}");
    assert_eq!(entry["liveness_source"], json!("channel"), "{entry}");
    assert_eq!(
        entry["liveness"],
        json!("stale"),
        "a heartbeat 300s old against a 20s interval is 15x overdue, past the 3x threshold: {entry}"
    );

    handle.shutdown.cancel();
    handle.test_db.cleanup().await?;
    Ok(())
}
