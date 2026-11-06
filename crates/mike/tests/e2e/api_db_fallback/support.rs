use crate::anthropic_fixtures;
use crate::db_cap_helpers;

use anthropic_fixtures::RequestBody;
use db_cap_helpers::{TestDb, TestDbHandle};
use serde_json::Value;

pub(super) async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    user_text: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .max_tokens(4)
        .content(user_text)
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
    let _ = response.bytes().await?;
    Ok(())
}

pub(super) async fn wait_until_terminal(
    client: &reqwest::Client,
    inspect_url: &str,
    id: u64,
) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let list: Value = client
            .get(format!("{inspect_url}/api/requests"))
            .send()
            .await?
            .json()
            .await?;
        let status_for_id = list
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|r| r.get("id").and_then(|v| v.as_u64()) == Some(id))
            })
            .and_then(|r| r.get("status"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        match status_for_id.as_deref() {
            Some(s)
                if s.eq_ignore_ascii_case("complete") || s.to_lowercase().starts_with("error") =>
            {
                return Ok(());
            }
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "request id {id} did not reach terminal status within 2s; last seen status = {status_for_id:?}"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub(super) async fn wait_for_in_memory_eviction(
    client: &reqwest::Client,
    inspect_url: &str,
    evicted_id: u64,
) -> anyhow::Result<Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let snap: Value = client
            .get(format!("{inspect_url}/api/diagnostics/memory?top=10"))
            .send()
            .await?
            .json()
            .await?;
        let still_in_memory = snap
            .get("top_entries")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| r.get("id").and_then(|v| v.as_u64()))
                    .any(|id| id == evicted_id)
            })
            .unwrap_or(false);
        if !still_in_memory {
            return Ok(snap);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(snap);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

pub(super) struct CappedProxy {
    pub(super) proxy_url: String,
    pub(super) inspect_url: String,
    pub(super) shutdown: tokio_util::sync::CancellationToken,
    pub(super) test_db: TestDbHandle,
}

pub(super) async fn boot_small_cap_proxy(mock_uri: &str) -> anyhow::Result<Option<CappedProxy>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let (proxy_url, inspect_url, shutdown) =
        db_cap_helpers::start_proxy_with_db_at_capped(mock_uri, &test_db.database_url, 1).await?;

    Ok(Some(CappedProxy {
        proxy_url,
        inspect_url,
        shutdown,
        test_db,
    }))
}
