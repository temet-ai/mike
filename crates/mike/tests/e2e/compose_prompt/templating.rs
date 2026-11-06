use crate::common;
use crate::db_helpers;
use crate::upstream;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::support::{
    insert_composition, latest_request_id_for_session, prompt_sha256, send_canonical_request,
    send_non_text_block_request, wait_for_request_detail,
};

#[tokio::test]
async fn template_substitutes_block_2_preserving_cache_control() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let block2_in = "role-body-incoming";
    let new_block2 = "REWRITTEN-ROLE-BODY";
    insert_composition(
        &pool,
        &prompt_sha256(block2_in),
        "main-agent",
        new_block2,
        "template",
    )
    .await?;

    let (proxy_url, _inspect_url, _shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();
    let session_id = "deadbeef-abcd-4ef0-9876-543210000001";

    let r = send_canonical_request(&client, &proxy_url, session_id, block2_in).await?;
    assert_eq!(r.status(), 200);

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 1);
    let upstream_body: Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(
        upstream_body["system"][2]["text"].as_str(),
        Some(new_block2),
        "block 2 text must be the composition's"
    );
    assert_eq!(
        upstream_body["system"][2]["cache_control"]["type"].as_str(),
        Some("ephemeral"),
        "cache_control type preserved"
    );
    assert_eq!(
        upstream_body["system"][2]["cache_control"]["ttl"].as_str(),
        Some("1h"),
        "cache_control ttl preserved verbatim from incoming request"
    );
    assert_eq!(
        upstream_body["system"][0]["text"].as_str(),
        Some("billing-header"),
    );
    assert_eq!(
        upstream_body["system"][1]["cache_control"]["type"].as_str(),
        Some("ephemeral"),
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn composition_disabled_globally_is_passthrough() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let block2_in = "role-body-killed";
    insert_composition(
        &pool,
        &prompt_sha256(block2_in),
        "main-agent",
        "REWRITTEN",
        "template",
    )
    .await?;

    let config = mike::config::ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream.uri().as_str(),
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &test_db.database_url,
        "--composition-enabled",
        "0",
    ])?;
    let state = mike::config::AppState::new(config).await?;
    let shutdown = state.shutdown();
    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = mike::app::proxy_router(state.clone());
    let inspect_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let inspect_app = mike::app::inspect_router_on(state, inspect_listener.local_addr()?);
    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });
    let proxy_url = format!("http://{proxy_addr}");

    let client = reqwest::Client::new();
    let session_id = "deadbeef-2222-4444-8888-555555555555";
    let r = send_canonical_request(&client, &proxy_url, session_id, block2_in).await?;
    assert_eq!(r.status(), 200);

    let received = upstream.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("no requests"))?;
    assert_eq!(received.len(), 1);
    let upstream_body: Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(
        upstream_body["system"][2]["text"].as_str(),
        Some(block2_in),
        "kill switch preserves byte-identity"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn prompt_sha256_matches_sha256_hex_of_block_2_text() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let (proxy_url, inspect_url) =
        common::start_proxy_with_composition_enabled(&upstream.uri()).await?;
    let client = reqwest::Client::new();
    let session_id = "11111111-2222-4333-8444-555555555555";
    let block2 = "deterministic-role-body";
    let expected = {
        let mut hasher = Sha256::new();
        hasher.update(block2.as_bytes());
        hex::encode(hasher.finalize())
    };

    let r = send_canonical_request(&client, &proxy_url, session_id, block2).await?;
    assert_eq!(r.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = wait_for_request_detail(&client, &inspect_url, request_id).await?;
    let actual = detail
        .get("system_blocks")
        .and_then(|v| v.get("prompt_sha256"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("prompt_sha256 absent"))?;
    assert_eq!(actual, expected);
    Ok(())
}

#[tokio::test]
async fn unknown_blocks_logged_to_request_row() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let (proxy_url, inspect_url) =
        common::start_proxy_with_composition_enabled(&upstream.uri()).await?;
    let client = reqwest::Client::new();
    let session_id = "ffeeffee-1111-4222-8333-444444444444";

    let r = send_non_text_block_request(&client, &proxy_url, session_id).await?;
    assert_eq!(r.status(), 200);

    let request_id = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let detail = wait_for_request_detail(&client, &inspect_url, request_id).await?;
    let unknown = detail
        .get("unknown_blocks")
        .ok_or_else(|| anyhow::anyhow!("unknown_blocks absent"))?;
    assert_eq!(
        unknown.get("reason").and_then(|v| v.as_str()),
        Some("non_text_block")
    );
    Ok(())
}

#[tokio::test]
async fn composition_applied_persisted_correctly() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let block2_in = "role-body-applied-test";
    insert_composition(
        &pool,
        &prompt_sha256(block2_in),
        "main-agent",
        "REWRITTEN-APPLIED",
        "template",
    )
    .await?;

    let (proxy_url, inspect_url, _shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();
    let session_id = "deadbeef-3333-4444-8888-666666666666";

    let r1 = send_canonical_request(&client, &proxy_url, session_id, block2_in).await?;
    assert_eq!(r1.status(), 200);
    let id1 = latest_request_id_for_session(&client, &inspect_url, session_id).await?;
    let d1 = wait_for_request_detail(&client, &inspect_url, id1).await?;
    assert_eq!(
        d1.get("composition_applied"),
        Some(&Value::Bool(true)),
        "first request must have composition_applied=true"
    );
    let prompt_sha256_value_1 = d1
        .get("system_blocks")
        .and_then(|v| v.get("prompt_sha256"))
        .and_then(|v| v.as_str())
        .map(String::from);
    assert_eq!(
        prompt_sha256_value_1.as_deref(),
        Some(prompt_sha256(block2_in).as_str())
    );

    let r2 = send_canonical_request(&client, &proxy_url, session_id, block2_in).await?;
    assert_eq!(r2.status(), 200);
    let summaries: Vec<Value> = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let id2 = summaries
        .iter()
        .filter(|s| s.get("session_id").and_then(|v| v.as_str()) == Some(session_id))
        .filter_map(|s| s.get("id").and_then(|v| v.as_u64()))
        .max()
        .ok_or_else(|| anyhow::anyhow!("no requests for session"))?;
    assert_ne!(id2, id1, "second request must have a fresh memory id");
    let d2 = wait_for_request_detail(&client, &inspect_url, id2).await?;
    assert_eq!(
        d2.get("composition_applied"),
        Some(&Value::Bool(false)),
        "second request in session must not rewrite"
    );
    let prompt_sha256_value_2 = d2
        .get("system_blocks")
        .and_then(|v| v.get("prompt_sha256"))
        .and_then(|v| v.as_str())
        .map(String::from);
    assert_eq!(
        prompt_sha256_value_2.as_deref(),
        Some(prompt_sha256(block2_in).as_str()),
        "continuation persists the same capture-only prompt_sha256 as its \
         PrimaryFirst head (dedups to one system_prompts row), without rewriting"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
