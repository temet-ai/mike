use crate::common;

use crate::db_helpers;

use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn post_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    model: &str,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .json(
            &RequestBody::default()
                .model(model)
                .content("turn")
                .max_tokens(1)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

async fn poll_until_sessions_hold_requests(
    client: &reqwest::Client,
    inspect_url: &str,
    expected_count: usize,
    expected_requests: u64,
    expected_leading_requests: u64,
) -> anyhow::Result<Vec<Value>> {
    let mut last: Vec<Value> = Vec::new();
    for _ in 0..40 {
        let resp = client
            .get(format!("{inspect_url}/api/sessions"))
            .send()
            .await?;
        anyhow::ensure!(resp.status() == 200);
        let body: Value = resp.json().await?;
        let arr = body
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("sessions must be an array"))?
            .clone();
        let requests: u64 = arr
            .iter()
            .filter_map(|row| row["request_count"].as_u64())
            .sum();
        let leading = arr.first().and_then(|row| row["request_count"].as_u64());
        if arr.len() == expected_count
            && requests == expected_requests
            && leading == Some(expected_leading_requests)
        {
            return Ok(arr);
        }
        last = arr;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!(
        "timed out waiting for {expected_count} sessions holding {expected_requests} requests with {expected_leading_requests} in the leading one, last listing: {last:?}"
    )
}

async fn wait_until_this_clock_passes_the_session_row(
    pool: &sqlx::PgPool,
    session: &str,
) -> anyhow::Result<()> {
    for _ in 0..200 {
        let seen: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT last_seen FROM sessions.sessions
              WHERE session_uuid = $1::text::uuid AND last_status = 'Complete'",
        )
        .bind(session)
        .fetch_optional(pool)
        .await?;
        if seen.is_some_and(|seen| chrono::Utc::now() > seen) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    anyhow::bail!("session {session} never reached a last_seen this clock has passed")
}

fn assert_sorted_by_last_seen_desc(rows: &[Value]) -> anyhow::Result<()> {
    let first = &rows[0];
    let second = &rows[1];
    assert_eq!(
        first["request_count"], 3,
        "leading row should be the 3-turn Sess A session"
    );
    assert_eq!(
        second["request_count"], 1,
        "trailing row should be the 1-turn Sess B session"
    );
    assert!(
        first["last_seen"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("last_seen must be a string"))?
            >= second["last_seen"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("last_seen must be a string"))?,
        "rows must be sorted by last_seen descending"
    );
    Ok(())
}

fn assert_session_list_shape(rows: &[Value], sess_a: &str, sess_b: &str) -> anyhow::Result<()> {
    let first = &rows[0];

    let models = first["models"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("models must be an array"))?;
    let model_strs: Vec<&str> = models.iter().filter_map(Value::as_str).collect();
    assert!(
        model_strs.is_empty(),
        "session-list `models` is intentionally empty since c01d9232 (the \
         per-session models scan was dropped); got {model_strs:?}"
    );

    for required in [
        "id",
        "first_seen",
        "last_seen",
        "request_count",
        "models",
        "last_status",
        "project",
    ] {
        assert!(
            first.get(required).is_some(),
            "session row missing required key `{required}`: {first}"
        );
    }

    for row in rows {
        assert_eq!(
            row["project"],
            Value::Null,
            "session row `project` must be null when no CLAUDE.md path appears \
             in any request's system prompt, got: {row}"
        );
    }

    let ids: Vec<&str> = rows.iter().filter_map(|row| row["id"].as_str()).collect();
    assert!(
        ids.contains(&sess_a),
        "session A's UUID must appear verbatim, got {ids:?}"
    );
    assert!(
        ids.contains(&sess_b),
        "session B's UUID must appear verbatim, got {ids:?}"
    );
    Ok(())
}

#[tokio::test]
async fn test_list_sessions_groups_and_dedups_models() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_sess_list")
                        .text("ok")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let proxy = db_helpers::start_proxy_with_db(&mock_server.uri())
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("MIKE_TEST_ADMIN_URL must be set; DB is required for this test")
        })?;
    let (proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let sess_a = "11111111-1111-4111-8111-000000000001";
    let sess_b = "22222222-2222-4222-8222-000000000002";

    post_request(&client, &proxy_url, sess_a, "claude-sonnet-4-20250514").await?;
    post_request(&client, &proxy_url, sess_a, "claude-opus-4-5-20250929").await?;

    post_request(&client, &proxy_url, sess_b, "claude-sonnet-4-20250514").await?;
    wait_until_this_clock_passes_the_session_row(&pool, sess_b).await?;

    post_request(&client, &proxy_url, sess_a, "claude-sonnet-4-20250514").await?;

    let rows = poll_until_sessions_hold_requests(&client, &inspect_url, 2, 4, 3).await?;

    assert_sorted_by_last_seen_desc(&rows)?;
    assert_session_list_shape(&rows, sess_a, sess_b)?;

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn test_legacy_conversations_routes_return_404() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    for url in [
        format!("{inspect_url}/api/conversations"),
        format!("{inspect_url}/api/conversations/anything"),
    ] {
        let resp = client.get(&url).send().await?;
        assert_eq!(
            resp.status(),
            404,
            "legacy conversations URL must 404 after the rename, got {} for {url}",
            resp.status()
        );
    }

    Ok(())
}
