use anyhow::{Context, Result};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::common::*;
use crate::harness::{INTERCEPT_HOST, spawn_forward, wait_until_probed, write_extra_ca};

#[tokio::test]
async fn websocket_upgrade_bridges_through_direct_leg_and_v1_still_routes() -> Result<()> {
    let routed = plain_http_stub("routed-ok").await?;
    let health = plain_http_stub("{\"status\":\"ok\"}").await?;
    let (upstream_addr, ca_pem) = tls_upgrade_stub(INTERCEPT_HOST).await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let proxy = spawn_forward(
        format!("http://{}", routed.addr),
        format!("http://{}/api/health", health.addr),
        Some(extra_ca),
        Some(upstream_addr.to_string()),
        Duration::from_millis(50),
    )
    .await?;

    wait_until_probed(&health).await?;

    let mut handshake =
        connect_and_upgrade(proxy.addr, INTERCEPT_HOST, &proxy.ca_der, "/v1/control").await?;

    assert_eq!(
        handshake.status, 101,
        "upstream's 101 Switching Protocols must be relayed to the client verbatim"
    );
    let upgrade_header = handshake
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("upgrade"))
        .map(|(_, v)| v.as_str());
    assert_eq!(
        upgrade_header,
        Some("websocket"),
        "upstream's Upgrade header must be relayed on the 101 response"
    );

    let payload = b"remote-control channel payload, both directions";
    handshake
        .upgraded
        .write_all(payload)
        .await
        .context("write to bridged upgraded stream")?;
    let mut echoed = vec![0u8; payload.len()];
    handshake
        .upgraded
        .read_exact(&mut echoed)
        .await
        .context("read echo from bridged upgraded stream")?;
    assert_eq!(
        &echoed, payload,
        "bytes must round-trip bidirectionally through the bridge"
    );

    let normal = connect_and_request(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "POST",
        "/v1/messages",
        b"{\"ok\":true}",
    )
    .await?;
    assert_eq!(normal.status, 200);
    assert_eq!(
        normal.body, b"routed-ok",
        "an ordinary /v1/messages request still routes through :9988 unchanged"
    );

    let routed_received = routed.received.lock().await;
    assert_eq!(
        routed_received.len(),
        1,
        "only the ordinary /v1/messages request reached the routed leg — \
         the upgrade request must never have routed there, even though it \
         named a /v1/* path and health was Up"
    );
    assert_eq!(routed_received[0].path, "/v1/messages");

    proxy.cancel.cancel();
    Ok(())
}
