use anyhow::Result;
use std::time::Duration;

use crate::common::*;
use crate::harness::{INTERCEPT_HOST, closed_health_url, spawn_forward_capped, write_extra_ca};

#[tokio::test]
async fn over_cap_body_yields_413() -> Result<()> {
    let (anthropic, ca_pem) = tls_http_stub(INTERCEPT_HOST, "direct-ok").await?;
    let (_extra_dir, extra_ca) = write_extra_ca(&ca_pem)?;

    let proxy = spawn_forward_capped(
        "http://127.0.0.1:1".to_string(),
        closed_health_url(),
        Some(extra_ca),
        Some(anthropic.addr.to_string()),
        Duration::from_secs(60),
        16,
    )
    .await?;

    let big = vec![b'x'; 64];
    let resp = connect_and_request_owned(
        proxy.addr,
        INTERCEPT_HOST,
        &proxy.ca_der,
        "POST",
        "/v1/messages",
        big,
    )
    .await?;

    assert_eq!(
        resp.status, 413,
        "over-cap body must be rejected with 413 Payload Too Large"
    );
    let received = anthropic.received.lock().await;
    assert!(
        received.is_empty(),
        "an over-cap request must never reach an upstream"
    );

    proxy.cancel.cancel();
    Ok(())
}
