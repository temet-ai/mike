use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::{RequestBody, text_block};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mock_upstream() -> MockServer {
    let mock_server = MockServer::start().await;
    let messages_response = ResponseTemplate::new(200)
        .set_body_json(json!({
            "id": "msg_eligibility",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "ack"}],
            "model": "claude-haiku-4-5-20251001",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }))
        .insert_header("content-type", "application/json");
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(messages_response)
        .mount(&mock_server)
        .await;

    let count_response = ResponseTemplate::new(200)
        .set_body_json(json!({"input_tokens": 7}))
        .insert_header("content-type", "application/json");
    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .respond_with(count_response)
        .mount(&mock_server)
        .await;

    mock_server
}

async fn wait_for_rsp_row(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> anyhow::Result<(Option<Value>, Option<String>)> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let row: Option<(Option<Value>, Option<String>)> = sqlx::query_as(
            "SELECT rsp.unknown_blocks, rsp.prompt_sha256
             FROM requests.system_prompts rsp
             JOIN requests.requests r ON r.id = rsp.request_id
             JOIN sessions.sessions s ON s.id = r.session_id
             WHERE s.session_uuid = $1::uuid
             ORDER BY r.id DESC
             LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
        if let Some(row) = row {
            return Ok(row);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for request_system_prompts row on session {session_id}"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

async fn send_quota_probe(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .model("claude-haiku-4-5-20251001")
        .max_tokens(1)
        .content("quota")
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "expected 200 OK, got {}",
        resp.status()
    );
    Ok(())
}

async fn send_count_tokens(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let body = json!({
        "model": "claude-sonnet-4-20250514",
        "messages": [{"role": "user", "content": "how many tokens?"}]
    });
    let resp = client
        .post(format!("{proxy_url}/v1/messages/count_tokens"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "expected 200 OK, got {}",
        resp.status()
    );
    Ok(())
}

async fn send_classifiable_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .system(json!([
            text_block("billing-header"),
            text_block("identity preamble"),
            text_block("novel role body for eligibility positive control"),
        ]))
        .content("hi")
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "expected 200 OK, got {}",
        resp.status()
    );
    Ok(())
}

#[tokio::test]
async fn quota_probe_does_not_populate_anomaly_fields() -> anyhow::Result<()> {
    let upstream = mock_upstream().await;
    let Some(handle) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = handle.into_parts();
    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_quota_probe(&client, &proxy_url, &session_id).await?;
    let (unknown_blocks, prompt_sha256) = wait_for_rsp_row(&pool, &session_id).await?;
    assert!(
        unknown_blocks.is_none(),
        "quota probe must NOT carry unknown_blocks; got {unknown_blocks:?}"
    );
    assert!(
        prompt_sha256.is_none(),
        "quota probe must NOT carry prompt_sha256; got {prompt_sha256:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn count_tokens_does_not_populate_anomaly_fields() -> anyhow::Result<()> {
    let upstream = mock_upstream().await;
    let Some(handle) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = handle.into_parts();
    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_count_tokens(&client, &proxy_url, &session_id).await?;
    let (unknown_blocks, prompt_sha256) = wait_for_rsp_row(&pool, &session_id).await?;
    assert!(
        unknown_blocks.is_none(),
        "count_tokens request must NOT carry unknown_blocks; got {unknown_blocks:?}"
    );
    assert!(
        prompt_sha256.is_none(),
        "count_tokens request must NOT carry prompt_sha256; got {prompt_sha256:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn eligible_request_still_classifies_after_gate() -> anyhow::Result<()> {
    let upstream = mock_upstream().await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, _inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_classifiable_request(&client, &proxy_url, &session_id).await?;
    let (unknown_blocks, prompt_sha256) = wait_for_rsp_row(&pool, &session_id).await?;
    assert!(
        unknown_blocks.is_none(),
        "classifiable request must not carry unknown_blocks; got {unknown_blocks:?}"
    );
    let sha = prompt_sha256
        .ok_or_else(|| anyhow::anyhow!("classifiable request must carry prompt_sha256"))?;
    assert_eq!(
        sha.len(),
        64,
        "prompt_sha256 must be 64 hex chars; got {sha}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn continuation_persists_system_prompt_identity() -> anyhow::Result<()> {
    let upstream = mock_upstream().await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, _inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();

    send_classifiable_request(&client, &proxy_url, &session_id).await?;
    send_classifiable_request(&client, &proxy_url, &session_id).await?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM requests.system_prompts rsp
             JOIN requests.requests r ON r.id = rsp.request_id
             JOIN sessions.sessions s ON s.id = r.session_id
             WHERE s.session_uuid = $1::uuid",
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await?;
        if count >= 2 {
            break;
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "timed out waiting for 2 rsp rows on session {session_id} (got {count})"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let (unknown_blocks, prompt_sha256): (Option<Value>, Option<String>) = sqlx::query_as(
        "SELECT rsp.unknown_blocks, rsp.prompt_sha256
         FROM requests.system_prompts rsp
         JOIN requests.requests r ON r.id = rsp.request_id
         JOIN sessions.sessions s ON s.id = r.session_id
         WHERE s.session_uuid = $1::uuid
         ORDER BY r.id DESC
         LIMIT 1",
    )
    .bind(&session_id)
    .fetch_one(&pool)
    .await?;

    assert!(
        unknown_blocks.is_none(),
        "continuation must not carry unknown_blocks; got {unknown_blocks:?}"
    );
    let sha = prompt_sha256.ok_or_else(|| {
        anyhow::anyhow!(
            "continuation (non-PrimaryFirst) must carry prompt_sha256 — Fix 1 regressed"
        )
    })?;
    assert_eq!(
        sha.len(),
        64,
        "prompt_sha256 must be 64 hex chars; got {sha}"
    );

    let blocks: Option<Value> = sqlx::query_scalar(
        "SELECT blocks FROM system_prompts.system_prompts WHERE prompt_sha256 = $1",
    )
    .bind(&sha)
    .fetch_optional(&pool)
    .await?;
    let blocks = blocks.ok_or_else(|| {
        anyhow::anyhow!("system_prompts row must exist for the continuation's sha {sha}")
    })?;
    assert!(
        blocks.is_array(),
        "system_prompts.blocks must be the captured system array; got {blocks:?}"
    );

    let distinct_shas: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT rsp.prompt_sha256)
         FROM requests.system_prompts rsp
         JOIN requests.requests r ON r.id = rsp.request_id
         JOIN sessions.sessions s ON s.id = r.session_id
         WHERE s.session_uuid = $1::uuid
           AND rsp.prompt_sha256 IS NOT NULL",
    )
    .bind(&session_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        distinct_shas, 1,
        "PrimaryFirst head and its continuation must dedup to ONE sha \
         (capture-only identity == classifier identity); got {distinct_shas}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
