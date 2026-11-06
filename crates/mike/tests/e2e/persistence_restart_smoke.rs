use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    user_text: &str,
) -> anyhow::Result<bytes::Bytes> {
    let body = RequestBody::default()
        .content(user_text)
        .max_tokens(16)
        .build();
    let response = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        response.status() == 200,
        "expected 200 OK, got {}",
        response.status()
    );
    Ok(response.bytes().await?)
}

async fn fetch_request_body_presence(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> anyhow::Result<Vec<(i64, bool)>> {
    let rows: Vec<(i64, bool)> = sqlx::query_as(
        "SELECT r.id, (r.request_body IS NOT NULL) AS has_body
         FROM requests.requests r
         JOIN sessions.sessions s ON s.id = r.session_id
         WHERE s.session_uuid = $1::uuid
         ORDER BY r.created_at ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn mount_ack_upstream() -> MockServer {
    let mock_server = MockServer::start().await;
    let response_body = MockResponse::new("msg_restart")
        .text("ack")
        .usage(5, 1)
        .build();
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&response_body)
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;
    mock_server
}

async fn pin_session(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let pin_response = client
        .patch(format!("{inspect_url}/api/sessions/{session_id}/pin"))
        .json(&json!({ "pinned": true }))
        .send()
        .await?;
    anyhow::ensure!(
        pin_response.status() == 200,
        "pin endpoint returned {}",
        pin_response.status()
    );
    Ok(())
}

async fn fetch_session_detail(
    client: &reqwest::Client,
    inspect_url: &str,
    session_id: &str,
    expected_count: usize,
    count_label: &str,
) -> anyhow::Result<(serde_json::Value, Vec<u64>)> {
    let detail: serde_json::Value = client
        .get(format!("{inspect_url}/api/sessions/{session_id}"))
        .send()
        .await?
        .json()
        .await?;
    let requests = detail
        .get("requests")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    assert_eq!(
        requests.len(),
        expected_count,
        "expected {expected_count} {count_label}, got {}",
        requests.len()
    );
    let ids: Vec<u64> = requests
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_u64()))
        .collect();
    Ok((detail, ids))
}

fn assert_still_pinned(detail: &serde_json::Value) -> anyhow::Result<()> {
    let pinned = detail
        .get("summary")
        .and_then(|s| s.get("pinned"))
        .and_then(|v| v.as_bool())
        .ok_or_else(|| anyhow::anyhow!("summary.pinned missing in rehydrated detail"))?;
    assert!(pinned, "rehydrated session must still report pinned=true");
    Ok(())
}

#[tokio::test]
async fn persistence_restart_smoke() -> anyhow::Result<()> {
    let mock_server = mount_ack_upstream().await;

    let Some(handle) = db_helpers::start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping restart-smoke");
        return Ok(());
    };
    let (proxy_url_1, inspect_url_1, shutdown_1, pool, test_db) = handle.into_parts();

    let smoke_uuid = uuid::Uuid::new_v4();
    let session_id = smoke_uuid.to_string();

    let client = reqwest::Client::new();
    for user_text in ["first", "second", "third"] {
        send_request(&client, &proxy_url_1, &session_id, user_text).await?;
    }

    pin_session(&client, &inspect_url_1, &session_id).await?;

    let (_detail_before, ids_before) = fetch_session_detail(
        &client,
        &inspect_url_1,
        &session_id,
        3,
        "requests pre-restart",
    )
    .await?;
    assert_eq!(ids_before.len(), 3);

    let presence_before = fetch_request_body_presence(&pool, &session_id).await?;
    assert_eq!(presence_before.len(), 3, "3 persisted rows expected");
    for (id, has_body) in &presence_before {
        assert!(has_body, "request_body must be non-NULL for id {id}");
    }

    shutdown_1.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let database_url = test_db.database_url.clone();
    let (_proxy_url_2, inspect_url_2, shutdown_2) =
        db_helpers::start_proxy_with_db_at(&mock_server.uri(), &database_url).await?;

    let (detail_after, ids_after) = fetch_session_detail(
        &client,
        &inspect_url_2,
        &session_id,
        3,
        "rehydrated requests post-restart",
    )
    .await?;
    assert_eq!(
        ids_after, ids_before,
        "rehydrated request ids must match pre-restart ids"
    );

    assert_still_pinned(&detail_after)?;

    let presence_after = fetch_request_body_presence(&pool, &session_id).await?;
    assert_eq!(
        presence_after, presence_before,
        "body presence must not change across restart"
    );

    shutdown_2.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
