use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::RequestBody;
use serde_json::json;
use sqlx::AssertSqlSafe;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use db_helpers::start_proxy_with_db;

async fn install_fail_once_trigger(pool: &sqlx::PgPool, target_seq_ord: i32) -> anyhow::Result<()> {
    sqlx::query("CREATE SEQUENCE test_stream_block_fail_attempts")
        .execute(pool)
        .await?;
    sqlx::query(AssertSqlSafe(format!(
        "CREATE OR REPLACE FUNCTION test_fail_stream_block_once() RETURNS trigger AS $$
         DECLARE
           attempt bigint;
         BEGIN
           IF NEW.seq_ord = {target_seq_ord} THEN
             attempt := nextval('test_stream_block_fail_attempts');
             IF attempt = 1 THEN
               RAISE EXCEPTION 'injected test failure for seq_ord=% (attempt %)', NEW.seq_ord, attempt;
             END IF;
           END IF;
           RETURN NEW;
         END;
         $$ LANGUAGE plpgsql"
    )))
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TRIGGER test_fail_stream_block_once_trg
             BEFORE INSERT ON responses.stream_blocks
             FOR EACH ROW EXECUTE FUNCTION test_fail_stream_block_once()",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn install_fail_always_trigger(
    pool: &sqlx::PgPool,
    target_seq_ord: i32,
) -> anyhow::Result<()> {
    sqlx::query(AssertSqlSafe(format!(
        "CREATE OR REPLACE FUNCTION test_fail_stream_block_always() RETURNS trigger AS $$
         BEGIN
           IF NEW.seq_ord = {target_seq_ord} THEN
             RAISE EXCEPTION 'injected permanent test failure for seq_ord=%', NEW.seq_ord;
           END IF;
           RETURN NEW;
         END;
         $$ LANGUAGE plpgsql"
    )))
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TRIGGER test_fail_stream_block_always_trg
             BEFORE INSERT ON responses.stream_blocks
             FOR EACH ROW EXECUTE FUNCTION test_fail_stream_block_always()",
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn sse_three_block_body() -> String {
    let mut body = String::new();
    body.push_str("event: message_start\ndata: ");
    body.push_str(
        &json!({
            "type": "message_start",
            "message": {
                "id": "msg_gap_test",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 10, "output_tokens": 0}
            }
        })
        .to_string(),
    );
    body.push_str("\n\n");

    body.push_str("event: content_block_start\ndata: ");
    body.push_str(
        &json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_delta\ndata: ");
    body.push_str(
        &json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello "}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_stop\ndata: ");
    body.push_str(&json!({"type":"content_block_stop","index":0}).to_string());
    body.push_str("\n\n");

    body.push_str("event: content_block_start\ndata: ");
    body.push_str(
        &json!({"type":"content_block_start","index":1,"content_block":{"type":"thinking","thinking":""}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_delta\ndata: ");
    body.push_str(
        &json!({"type":"content_block_delta","index":1,"delta":{"type":"thinking_delta","thinking":"let me think"}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_delta\ndata: ");
    body.push_str(
        &json!({"type":"content_block_delta","index":1,"delta":{"type":"signature_delta","signature":"sig-test-abc"}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_stop\ndata: ");
    body.push_str(&json!({"type":"content_block_stop","index":1}).to_string());
    body.push_str("\n\n");

    body.push_str("event: content_block_start\ndata: ");
    body.push_str(
        &json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_delta\ndata: ");
    body.push_str(
        &json!({"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"world!"}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_stop\ndata: ");
    body.push_str(&json!({"type":"content_block_stop","index":2}).to_string());
    body.push_str("\n\n");

    body.push_str("event: message_delta\ndata: ");
    body.push_str(
        &json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens": 5}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: message_stop\ndata: ");
    body.push_str(&json!({"type":"message_stop"}).to_string());
    body.push_str("\n\n");
    body
}

fn sse_one_block_body() -> String {
    let mut body = String::new();
    body.push_str("event: message_start\ndata: ");
    body.push_str(
        &json!({
            "type": "message_start",
            "message": {
                "id": "msg_gap_test_single",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 10, "output_tokens": 0}
            }
        })
        .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_start\ndata: ");
    body.push_str(
        &json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_delta\ndata: ");
    body.push_str(
        &json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"unrecoverable"}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: content_block_stop\ndata: ");
    body.push_str(&json!({"type":"content_block_stop","index":0}).to_string());
    body.push_str("\n\n");
    body.push_str("event: message_delta\ndata: ");
    body.push_str(
        &json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens": 5}})
            .to_string(),
    );
    body.push_str("\n\n");
    body.push_str("event: message_stop\ndata: ");
    body.push_str(&json!({"type":"message_stop"}).to_string());
    body.push_str("\n\n");
    body
}

async fn post_stream(proxy_url: &str, client: &reqwest::Client) -> anyhow::Result<u64> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        )
        .json(
            &RequestBody::default()
                .stream(true)
                .content("stream me")
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "expected 200, got {}", resp.status());
    let bytes = resp.bytes().await?;
    let _ = bytes;
    Ok(0)
}

async fn wait_for_request_id(
    client: &reqwest::Client,
    inspect_url: &str,
    timeout: Duration,
) -> anyhow::Result<u64> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let list: serde_json::Value = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?
            .json()
            .await?;
        if let Some(id) = list
            .as_array()
            .and_then(|a| a.first())
            .and_then(|r| r["id"].as_u64())
        {
            return Ok(id);
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("timed out waiting for a captured request to appear");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_response_row(
    pool: &sqlx::PgPool,
    request_id: i64,
    timeout: Duration,
) -> anyhow::Result<bool> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let gapped: Option<(bool,)> = sqlx::query_as(
            "SELECT stream_blocks_gapped FROM responses.responses WHERE request_id = $1",
        )
        .bind(request_id)
        .fetch_optional(pool)
        .await?;
        if let Some((gapped,)) = gapped {
            return Ok(gapped);
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("timed out waiting for responses.responses row for request {request_id}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn stream_block_failed_once_is_recovered_by_terminal_ensure_retry() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(sse_three_block_body().as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };

    install_fail_once_trigger(&proxy.pool, 1).await?;

    let client = reqwest::Client::new();
    post_stream(&proxy.proxy_url, &client).await?;
    let id = wait_for_request_id(&client, &proxy.inspect_url, Duration::from_secs(5)).await?;
    let gapped = wait_for_response_row(&proxy.pool, id as i64, Duration::from_secs(10)).await?;

    assert!(
        !gapped,
        "a block recovered by the terminal ENSURE retry must not be reported gapped"
    );

    type Row = (i32, String, String);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT b.seq_ord, b.content, bt.code
           FROM responses.stream_blocks b
           JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
          WHERE b.request_id = $1
          ORDER BY b.seq_ord ASC",
    )
    .bind(id as i64)
    .fetch_all(&proxy.pool)
    .await?;

    assert_eq!(
        rows.len(),
        3,
        "all 3 blocks must be present after the retry — full seq_ord coverage: {rows:?}"
    );
    assert_eq!(rows[0], (0, "Hello ".to_string(), "text".to_string()));
    assert_eq!(
        rows[1],
        (1, "let me think".to_string(), "thinking".to_string()),
        "the block that failed its live write must be present with its full content after retry"
    );
    assert_eq!(rows[2], (2, "world!".to_string(), "text".to_string()));

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn stream_block_permanently_failing_sets_gapped_flag() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(sse_one_block_body().as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };

    install_fail_always_trigger(&proxy.pool, 0).await?;

    let client = reqwest::Client::new();
    post_stream(&proxy.proxy_url, &client).await?;
    let id = wait_for_request_id(&client, &proxy.inspect_url, Duration::from_secs(5)).await?;
    let gapped = wait_for_response_row(&proxy.pool, id as i64, Duration::from_secs(10)).await?;

    assert!(
        gapped,
        "a block that fails every attempt must be reported as a gap, not silently reconstructed around"
    );

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM responses.stream_blocks WHERE request_id = $1")
            .bind(id as i64)
            .fetch_one(&proxy.pool)
            .await?;
    assert_eq!(
        count.0, 0,
        "the unrecoverable block must genuinely be absent, not silently substituted"
    );

    let request_row_exists: (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM requests.requests WHERE id = $1)")
            .bind(id as i64)
            .fetch_one(&proxy.pool)
            .await?;
    assert!(
        request_row_exists.0,
        "a permanently-failing stream block retry must not roll back the request's own terminal write"
    );

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn stream_block_clean_response_is_not_gapped() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(sse_three_block_body().as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };

    let client = reqwest::Client::new();
    post_stream(&proxy.proxy_url, &client).await?;
    let id = wait_for_request_id(&client, &proxy.inspect_url, Duration::from_secs(5)).await?;
    let gapped = wait_for_response_row(&proxy.pool, id as i64, Duration::from_secs(10)).await?;

    assert!(
        !gapped,
        "a clean response with no write failures must never be reported gapped"
    );

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM responses.stream_blocks WHERE request_id = $1")
            .bind(id as i64)
            .fetch_one(&proxy.pool)
            .await?;
    assert_eq!(count.0, 3, "all 3 blocks must be present on the clean path");

    proxy.shutdown.cancel();
    proxy.test_db.cleanup().await?;
    Ok(())
}
