use anyhow::{Context, Result, anyhow};
use std::time::Duration;

use crate::common::*;
use crate::harness::{
    INTERCEPT_HOST, closed_direct_target, dummy_extra_ca, spawn_forward, wait_until_probed,
};

#[tokio::test]
async fn remote_control_sse_stream_relays_each_frame_through_routed_leg() -> Result<()> {
    const EVENTS: usize = 3;
    const GAP: Duration = Duration::from_millis(400);

    let routed = plain_sse_stub(EVENTS, GAP).await?;
    let health = plain_http_stub("{\"status\":\"ok\"}").await?;

    let (_extra_ca_dir, extra_ca) = dummy_extra_ca()?;
    let proxy = spawn_forward(
        format!("http://{}", routed.addr),
        format!("http://{}/api/health", health.addr),
        Some(extra_ca),
        Some(closed_direct_target()),
        Duration::from_millis(50),
    )
    .await?;
    wait_until_probed(&health).await?;

    let path = "/v1/code/sessions/sess-123/worker/events/stream";
    let mut resp = connect_and_stream(proxy.addr, INTERCEPT_HOST, &proxy.ca_der, path).await?;
    assert_eq!(resp.status(), 200, "routed SSE stream answered 200");
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream"),
        "SSE content-type relayed"
    );

    let mut arrivals: Vec<(std::time::Instant, bytes::Bytes)> = Vec::new();
    while let Some(frame) = http_body_util::BodyExt::frame(resp.body_mut()).await {
        let frame = frame.context("read frame")?;
        if let Ok(data) = frame.into_data() {
            arrivals.push((std::time::Instant::now(), data));
        }
    }

    let received: Vec<u8> = arrivals
        .iter()
        .flat_map(|(_, b)| b.iter().copied())
        .collect();
    let expected: String = (1..=EVENTS).map(sse_frame).collect();
    assert_eq!(
        String::from_utf8_lossy(&received),
        expected,
        "every SSE frame relayed byte-identical and in order"
    );

    let first = arrivals
        .first()
        .map(|(t, _)| *t)
        .ok_or_else(|| anyhow!("no frames arrived"))?;
    let last = arrivals
        .last()
        .map(|(t, _)| *t)
        .ok_or_else(|| anyhow!("no frames arrived"))?;
    let spread = last.duration_since(first);
    let min_spread = GAP * u32::try_from(EVENTS - 1).context("events fit u32")? / 2;
    assert!(
        spread >= min_spread,
        "frames arrived {spread:?} apart in total; a live relay spans >= {min_spread:?} \
         (the stub wrote them {GAP:?} apart) — the proxy held the stream until it ended"
    );

    let received = routed.received.lock().await;
    assert_eq!(received.len(), 1, "routed stub got exactly one request");
    let rec = &received[0];
    assert_eq!(rec.method, "GET");
    assert_eq!(rec.path, path);
    assert!(
        rec.has_header("accept", "text/event-stream"),
        "Accept forwarded to the routed leg"
    );
    assert!(
        rec.has_header("authorization", "Bearer sk-ant-oat-test"),
        "bearer token forwarded to the routed leg"
    );

    proxy.cancel.cancel();
    Ok(())
}
