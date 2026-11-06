use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use tokio::net::TcpListener;
use wiremock::MockServer;

async fn start_persist_disabled_proxy()
-> anyhow::Result<(String, tokio_util::sync::CancellationToken)> {
    let upstream = MockServer::start().await;
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        &upstream.uri(),
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "0",
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_app = proxy_router(state.clone());
    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);

    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _ = axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await;
    });
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _ = axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await;
    });

    Ok((format!("http://{inspect_addr}"), shutdown))
}

#[tokio::test]
async fn list_requests_bad_date_from_returns_400() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/requests?date_from=not-a-date"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 400, "bad date_from must return 400");
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_requests_bad_date_to_returns_400() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/requests?date_to=2026-13-99"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 400, "bad date_to must return 400");
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_requests_valid_date_from_no_db_returns_bare_array() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!(
            "{base}/api/requests?date_from=2020-01-01T00:00:00Z"
        ))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await?;
    assert!(
        body.is_array(),
        "list_requests no-DB response body must be a bare JSON array; got: {body:?}"
    );
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_requests_no_dates_no_db_returns_bare_array() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/requests"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await?;
    assert!(
        body.is_array(),
        "list_requests no-DB response body must be a bare JSON array (not envelope); got: {body:?}"
    );
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_sessions_bad_date_from_returns_400() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/sessions?date_from=garbage"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 400, "bad date_from must return 400");
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_sessions_bad_date_to_returns_400() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/sessions?date_to=notadate"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 400, "bad date_to must return 400");
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_sessions_valid_date_window_no_db_returns_empty_array() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!(
            "{base}/api/sessions?date_from=2020-01-01T00:00:00Z"
        ))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await?;
    assert!(
        body.is_array(),
        "list_sessions no-DB must return a JSON array; got: {body:?}"
    );
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_requests_no_dates_announces_default_7day_window() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/requests"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);
    let note = resp
        .headers()
        .get("x-window-note")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let note = note.ok_or_else(|| {
        anyhow::anyhow!("criterion 3: default window must emit X-Window-Note header")
    })?;
    assert!(
        note.contains("7 days"),
        "criterion 3: default window note must name the 7-day window; got: {note:?}"
    );
    assert!(
        note.contains("date_from"),
        "criterion 4: note must tell the caller how to widen (date_from); got: {note:?}"
    );
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_requests_explicit_date_from_announces_explicit_window() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!(
            "{base}/api/requests?date_from=2020-01-01T00:00:00Z"
        ))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);
    let note = resp
        .headers()
        .get("x-window-note")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("explicit window must still emit X-Window-Note header"))?;
    assert!(
        note.contains("2020-01-01"),
        "explicit window note must reflect the caller's date_from; got: {note:?}"
    );
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_sessions_no_dates_announces_default_7day_window() -> anyhow::Result<()> {
    let (base, shutdown) = start_persist_disabled_proxy().await?;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/sessions"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);
    let note = resp
        .headers()
        .get("x-window-note")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("criterion 3: default window must emit X-Window-Note header")
        })?;
    assert!(
        note.contains("7 days"),
        "criterion 3: default window note must name the 7-day window; got: {note:?}"
    );
    shutdown.cancel();
    Ok(())
}
