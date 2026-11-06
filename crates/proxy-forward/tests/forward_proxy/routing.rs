use anyhow::Result;

use crate::common::*;
use crate::harness::{
    INTERCEPT_HOST, dummy_extra_ca, spawn_forward, wait_until_probed, write_extra_ca,
};
use std::time::Duration;

#[tokio::test]
async fn mitm_routes_v1_to_proxy_when_up() -> Result<()> {
    let routed = plain_http_stub("routed-ok").await?;
    let health = plain_http_stub("{\"status\":\"ok\"}").await?;

    let (_extra_ca_dir, extra_ca) = dummy_extra_ca()?;
    let proxy = spawn_forward(
        format!("http://{}", routed.addr),
        format!("http://{}/api/health", health.addr),
        Some(extra_ca),
        None,
        Duration::from_millis(50),
    )
    .await?;

    wait_until_probed(&health).await?;

    let resp = connect_and_request(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "POST",
        "/v1/messages",
        b"{\"hello\":\"world\"}",
    )
    .await?;

    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.body, b"routed-ok",
        "response came from the routed stub"
    );

    let received = routed.received.lock().await;
    assert_eq!(received.len(), 1, "routed stub got exactly one request");
    let rec = &received[0];
    assert_eq!(rec.method, "POST");
    assert_eq!(rec.path, "/v1/messages");
    assert_eq!(rec.body, b"{\"hello\":\"world\"}", "body byte-identical");
    assert!(
        rec.has_header("x-api-key", "test-key"),
        "x-api-key forwarded to routed leg"
    );

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn mitm_routes_v2_to_proxy_when_up() -> Result<()> {
    let routed = plain_http_stub("routed-ok").await?;
    let health = plain_http_stub("{\"status\":\"ok\"}").await?;

    let (_extra_ca_dir, extra_ca) = dummy_extra_ca()?;
    let proxy = spawn_forward(
        format!("http://{}", routed.addr),
        format!("http://{}/api/health", health.addr),
        Some(extra_ca),
        None,
        Duration::from_millis(50),
    )
    .await?;

    wait_until_probed(&health).await?;

    let resp = connect_and_request(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "POST",
        "/v2/messages",
        b"{\"hello\":\"v2\"}",
    )
    .await?;

    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.body, b"routed-ok",
        "response came from the routed stub"
    );

    let received = routed.received.lock().await;
    assert_eq!(received.len(), 1, "routed stub got exactly one request");
    let rec = &received[0];
    assert_eq!(rec.method, "POST");
    assert_eq!(rec.path, "/v2/messages");
    assert_eq!(rec.body, b"{\"hello\":\"v2\"}", "body byte-identical");

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn mitm_serves_h2_client_and_routes() -> Result<()> {
    let routed = plain_http_stub("routed-ok").await?;
    let health = plain_http_stub("{\"status\":\"ok\"}").await?;

    let (_extra_ca_dir, extra_ca) = dummy_extra_ca()?;
    let proxy = spawn_forward(
        format!("http://{}", routed.addr),
        format!("http://{}/api/health", health.addr),
        Some(extra_ca),
        None,
        Duration::from_millis(50),
    )
    .await?;

    wait_until_probed(&health).await?;

    let (resp, alpn) = connect_and_request_h2(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "POST",
        "/v1/messages",
        b"{\"h2\":true}",
    )
    .await?;

    assert_eq!(alpn, b"h2", "client negotiated h2 with the MITM server");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"routed-ok");

    let received = routed.received.lock().await;
    assert_eq!(
        received.len(),
        1,
        "h2 request routed to the plain-HTTP stub"
    );
    assert_eq!(received[0].body, b"{\"h2\":true}");

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn pessimistic_start_routes_v1_direct() -> Result<()> {
    let routed = plain_http_stub("routed-ok").await?;
    let (anthropic, ca_pem) = tls_http_stub(INTERCEPT_HOST, "direct-ok").await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let proxy = spawn_forward(
        format!("http://{}", routed.addr),
        "http://127.0.0.1:1/api/health".to_string(),
        Some(extra_ca),
        Some(anthropic.addr.to_string()),
        Duration::from_secs(60),
    )
    .await?;

    let resp = connect_and_request(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "POST",
        "/v1/messages",
        b"{}",
    )
    .await?;

    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"direct-ok", "pessimistic start went Direct");

    let routed_received = routed.received.lock().await;
    assert!(
        routed_received.is_empty(),
        "pessimistic start must NOT route through the proxy (Unknown ⇒ Direct)"
    );
    let direct_received = anthropic.received.lock().await;
    assert_eq!(direct_received.len(), 1, "direct stub served the request");

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn non_v1_path_routes_direct_even_when_up() -> Result<()> {
    let routed = plain_http_stub("routed-ok").await?;
    let health = plain_http_stub("{\"status\":\"ok\"}").await?;
    let (anthropic, ca_pem) = tls_http_stub(INTERCEPT_HOST, "direct-ok").await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let proxy = spawn_forward(
        format!("http://{}", routed.addr),
        format!("http://{}/api/health", health.addr),
        Some(extra_ca),
        Some(anthropic.addr.to_string()),
        Duration::from_millis(50),
    )
    .await?;

    wait_until_probed(&health).await?;

    let resp = connect_and_request(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "GET",
        "/healthz",
        b"",
    )
    .await?;

    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"direct-ok", "non-/v1 path went Direct");
    let received = routed.received.lock().await;
    assert!(
        received.is_empty(),
        "non-/v1 path must bypass the routed proxy"
    );

    proxy.cancel.cancel();
    Ok(())
}
