use crate::common;

use std::time::Duration;

use reqwest::header::{ACCESS_CONTROL_ALLOW_ORIGIN, ORIGIN};
use wiremock::MockServer;

#[tokio::test]
async fn proxy_bind_rejects_api_paths() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let (proxy_url, _inspect_url) = common::start_proxy(&upstream.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{proxy_url}/api/requests"))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        404,
        "proxy bind must not expose /api/* routes"
    );
    Ok(())
}

#[tokio::test]
async fn inspect_bind_rejects_v1_paths() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&upstream.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{inspect_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        404,
        "inspect bind must not expose /v1/* routes"
    );
    let received = upstream.received_requests().await.unwrap_or_default();
    assert!(
        received.is_empty(),
        "inspect bind must not forward /v1/* upstream, got {} requests",
        received.len()
    );
    Ok(())
}

#[tokio::test]
async fn inspect_bind_serves_cors_headers() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&upstream.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/requests"))
        .header(ORIGIN, "http://127.0.0.1:9990")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    let allow_origin = resp
        .headers()
        .get(ACCESS_CONTROL_ALLOW_ORIGIN)
        .and_then(|v| v.to_str().ok());
    assert!(
        allow_origin.is_some(),
        "inspect bind must emit Access-Control-Allow-Origin, got headers: {:?}",
        resp.headers()
    );
    Ok(())
}

#[tokio::test]
async fn inspect_bind_serves_options_preflight() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&upstream.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .request(
            reqwest::Method::OPTIONS,
            format!("{inspect_url}/api/requests"),
        )
        .header(ORIGIN, "http://127.0.0.1:9990")
        .header("access-control-request-method", "GET")
        .send()
        .await?;

    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 204,
        "preflight must return 200 or 204, got {status}"
    );
    let allow_origin = resp
        .headers()
        .get(ACCESS_CONTROL_ALLOW_ORIGIN)
        .and_then(|v| v.to_str().ok());
    assert!(
        allow_origin.is_some(),
        "preflight response must include Access-Control-Allow-Origin"
    );
    Ok(())
}

#[tokio::test]
async fn both_binds_shutdown_together() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let (proxy_url, inspect_url, shutdown) =
        common::start_proxy_with_shutdown(&upstream.uri()).await?;
    let client = reqwest::Client::new();

    let pre_proxy = client
        .get(format!("{proxy_url}/api/requests"))
        .send()
        .await?;
    assert_eq!(pre_proxy.status(), 404);
    let pre_inspect = client
        .get(format!("{inspect_url}/api/health"))
        .send()
        .await?;
    assert_eq!(pre_inspect.status(), 200);

    shutdown.cancel();

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if std::time::Instant::now() > deadline {
            anyhow::bail!("binds did not shut down within 3 seconds");
        }
        let proxy_dead = client
            .get(format!("{proxy_url}/api/requests"))
            .timeout(Duration::from_millis(100))
            .send()
            .await
            .is_err();
        let inspect_dead = client
            .get(format!("{inspect_url}/api/health"))
            .timeout(Duration::from_millis(100))
            .send()
            .await
            .is_err();
        if proxy_dead && inspect_dead {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}
