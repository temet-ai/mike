use anyhow::{Context, Result};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::common::*;
use crate::harness::{
    INTERCEPT_HOST, closed_direct_target, closed_health_url, dummy_extra_ca, spawn_forward,
    wait_until_probed, write_extra_ca,
};

#[tokio::test]
async fn connect_ack_is_sent_before_tls() -> Result<()> {
    let (_extra_ca_dir, extra_ca) = dummy_extra_ca()?;
    let proxy = spawn_forward(
        "http://127.0.0.1:1".to_string(),
        closed_health_url(),
        Some(extra_ca),
        Some(closed_direct_target()),
        Duration::from_secs(60),
    )
    .await?;

    let mut tcp = TcpStream::connect(proxy.addr).await.context("connect")?;
    tcp.write_all(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\nHost: api.anthropic.com:443\r\n\r\n")
        .await
        .context("send CONNECT")?;

    let mut buf = [0u8; 64];
    let n = tcp.read(&mut buf).await.context("read ack")?;
    let ack = String::from_utf8_lossy(&buf[..n]);
    assert!(
        ack.starts_with("HTTP/1.1 200"),
        "expected 200 Connection Established, got: {ack}"
    );

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn blind_tunnel_passes_bytes_unmodified() -> Result<()> {
    let echo = tcp_echo_stub().await?;
    let (_extra_ca_dir, extra_ca) = dummy_extra_ca()?;
    let proxy = spawn_forward(
        "http://127.0.0.1:1".to_string(),
        closed_health_url(),
        Some(extra_ca),
        Some(closed_direct_target()),
        Duration::from_secs(60),
    )
    .await?;

    let mut tcp = TcpStream::connect(proxy.addr).await.context("connect")?;
    let connect = format!(
        "CONNECT 127.0.0.1:{} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
        echo.port()
    );
    tcp.write_all(connect.as_bytes())
        .await
        .context("send CONNECT")?;

    let mut ack = [0u8; 64];
    let n = tcp.read(&mut ack).await.context("read ack")?;
    assert!(String::from_utf8_lossy(&ack[..n]).starts_with("HTTP/1.1 200"));

    let payload = b"\x16\x03\x01 not real TLS, just opaque bytes \xff\x00";
    tcp.write_all(payload).await.context("write payload")?;
    let mut got = vec![0u8; payload.len()];
    tcp.read_exact(&mut got).await.context("read echo")?;
    assert_eq!(&got, payload, "blind tunnel must pass bytes unmodified");

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn failover_to_direct_when_routed_down() -> Result<()> {
    let (anthropic, ca_pem) = tls_http_stub(INTERCEPT_HOST, "direct-ok").await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let proxy = spawn_forward(
        "http://127.0.0.1:1".to_string(),
        closed_health_url(),
        Some(extra_ca),
        Some(anthropic.addr.to_string()),
        Duration::from_millis(50),
    )
    .await?;

    let resp = connect_and_request(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "POST",
        "/v1/messages",
        b"{\"k\":\"v\"}",
    )
    .await?;

    assert_eq!(resp.status, 200, "direct leg returned the stub's response");
    assert_eq!(resp.body, b"direct-ok", "response came from the TLS stub");

    let received = anthropic.received.lock().await;
    assert_eq!(received.len(), 1, "TLS stub got exactly one request");
    let rec = &received[0];
    assert_eq!(rec.path, "/v1/messages");
    assert_eq!(
        rec.body, b"{\"k\":\"v\"}",
        "body byte-identical on direct leg"
    );
    assert!(
        rec.has_header("host", INTERCEPT_HOST),
        "Host header is the real intercept host on the direct leg"
    );

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn routed_reset_after_accept_yields_502() -> Result<()> {
    let reset_addr = accept_then_reset_stub().await?;
    let (anthropic, ca_pem) = tls_http_stub(INTERCEPT_HOST, "direct-ok").await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let health = plain_http_stub("{\"status\":\"ok\"}").await?;
    let proxy = spawn_forward(
        format!("http://{reset_addr}"),
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
        "POST",
        "/v1/messages",
        b"{\"k\":\"v\"}",
    )
    .await?;

    assert_eq!(
        resp.status, 502,
        "routed leg reset-after-accept must yield 502, not a Direct replay"
    );
    let direct_received = anthropic.received.lock().await;
    assert!(
        direct_received.is_empty(),
        "502-on-ambiguous must NOT double-send to the Direct leg"
    );

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn event_stream_replayable_failure_skips_direct_replay() -> Result<()> {
    let (routed_addr, routed_cancel) = stoppable_accept_stub().await?;
    let health = plain_http_stub("{\"status\":\"ok\"}").await?;
    let (anthropic, ca_pem) = tls_http_stub(INTERCEPT_HOST, "direct-ok").await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let proxy = spawn_forward(
        format!("http://{routed_addr}"),
        format!("http://{}/api/health", health.addr),
        Some(extra_ca),
        Some(anthropic.addr.to_string()),
        Duration::from_millis(300),
    )
    .await?;
    wait_until_probed(&health).await?;

    routed_cancel.cancel();
    wait_until_unbound(routed_addr).await?;

    let path = "/v1/code/sessions/sess-269/worker/events/stream";
    let resp = connect_and_stream(proxy.addr, INTERCEPT_HOST, &proxy.ca_der, path).await?;

    assert_eq!(
        resp.status(),
        502,
        "an event-stream request must get 502, not a Direct replay"
    );
    let direct_received = anthropic.received.lock().await;
    assert!(
        direct_received.is_empty(),
        "the Direct leg must never see an event-stream request replayed after a \
         Replayable routed-leg failure"
    );

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn non_event_stream_replayable_failure_still_falls_back_to_direct() -> Result<()> {
    let (routed_addr, routed_cancel) = stoppable_accept_stub().await?;
    let health = plain_http_stub("{\"status\":\"ok\"}").await?;
    let (anthropic, ca_pem) = tls_http_stub(INTERCEPT_HOST, "direct-ok").await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let proxy = spawn_forward(
        format!("http://{routed_addr}"),
        format!("http://{}/api/health", health.addr),
        Some(extra_ca),
        Some(anthropic.addr.to_string()),
        Duration::from_millis(300),
    )
    .await?;
    wait_until_probed(&health).await?;

    routed_cancel.cancel();
    wait_until_unbound(routed_addr).await?;

    let resp = connect_and_request(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "POST",
        "/v1/messages",
        b"{\"k\":\"v\"}",
    )
    .await?;

    assert_eq!(
        resp.status, 200,
        "a non-event-stream request must still replay on Direct"
    );
    assert_eq!(
        resp.body, b"direct-ok",
        "response came from the Direct stub"
    );
    let direct_received = anthropic.received.lock().await;
    assert_eq!(
        direct_received.len(),
        1,
        "the Direct stub must have received exactly the replayed request"
    );

    proxy.cancel.cancel();
    Ok(())
}

#[tokio::test]
async fn event_stream_while_health_down_goes_routed_not_direct() -> Result<()> {
    let (routed_addr, routed_cancel) = stoppable_accept_stub().await?;
    routed_cancel.cancel();
    wait_until_unbound(routed_addr).await?;
    let (dead_health_addr, dead_health_cancel) = stoppable_accept_stub().await?;
    dead_health_cancel.cancel();
    wait_until_unbound(dead_health_addr).await?;
    let (anthropic, ca_pem) = tls_http_stub(INTERCEPT_HOST, "direct-ok").await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let proxy = spawn_forward(
        format!("http://{routed_addr}"),
        format!("http://{dead_health_addr}/api/health"),
        Some(extra_ca),
        Some(anthropic.addr.to_string()),
        Duration::from_millis(300),
    )
    .await?;

    let path = "/v1/code/sessions/sess-cutover/worker/events/stream";
    let resp = connect_and_stream(proxy.addr, INTERCEPT_HOST, &proxy.ca_der, path).await?;

    assert_eq!(
        resp.status(),
        502,
        "an event-stream request while health is down must get 502 from the routed leg"
    );
    let direct_received = anthropic.received.lock().await;
    assert!(
        direct_received.is_empty(),
        "the Direct leg must never see an event-stream request, even while health reads down"
    );

    proxy.cancel.cancel();
    Ok(())
}
