#![allow(dead_code)]

use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use mike::store::test_support::{TestDb, TestDbHandle};
use serde_json::{Value, json};
use tokio::net::TcpListener;

pub(crate) fn session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(crate) fn claude_response_json() -> Value {
    json!({
        "id": "msg_filter_test",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "Hi!"}],
        "model": "claude-sonnet-4-20250514",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 3}
    })
}

pub(crate) struct StartedFilterProxy {
    pub(crate) proxy_url: String,
    pub(crate) inspect_url: String,
    test_db: Option<TestDbHandle>,
    pub(crate) shutdown: tokio_util::sync::CancellationToken,
}

impl Drop for StartedFilterProxy {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

impl StartedFilterProxy {
    pub(crate) async fn cleanup(mut self) -> anyhow::Result<()> {
        self.shutdown.cancel();
        if let Some(db) = self.test_db.take() {
            db.cleanup().await?;
        }
        Ok(())
    }
}

pub(crate) async fn start_proxy_with_rules(
    upstream_url: &str,
    rules: &[serde_json::Value],
) -> anyhow::Result<Option<StartedFilterProxy>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = factory.acquire_isolated().await?;

    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        test_db.database_url.as_str(),
    ])?;

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state, inspect_addr);

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
    tokio::task::yield_now().await;

    let proxy_url = format!("http://{proxy_addr}");
    let inspect_url = format!("http://{inspect_addr}");

    let client = reqwest::Client::new();
    for rule in rules {
        let resp = client
            .post(format!("{inspect_url}/api/filter/rules"))
            .json(rule)
            .send()
            .await?;
        let status = resp.status();
        anyhow::ensure!(
            status == reqwest::StatusCode::CREATED,
            "filter rule seed must return 201, got {status}: {}",
            resp.text().await?
        );
    }

    Ok(Some(StartedFilterProxy {
        proxy_url,
        inspect_url,
        test_db: Some(test_db),
        shutdown,
    }))
}
