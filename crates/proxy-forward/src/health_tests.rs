use super::*;

#[test]
fn unknown_is_not_up() {
    let h = ProxyHealth::unknown();
    assert_eq!(h.get(), HealthState::Unknown);
    assert!(!h.is_up());
}

#[test]
fn set_up_then_is_up() {
    let h = ProxyHealth::unknown();
    h.set(HealthState::Up);
    assert!(h.is_up());
    h.set(HealthState::Down);
    assert!(!h.is_up());
}

#[test]
fn bypass_ledger_counts_and_resets_when_the_routed_leg_recovers() {
    let h = ProxyHealth::unknown();
    assert_eq!(h.note_bypass(), 1);
    assert_eq!(h.note_bypass(), 2);
    let mut failures = 0;
    record_probe_result(&h, &mut failures, true, "http://127.0.0.1:9989/api/health");
    assert!(h.is_up());
    assert_eq!(h.take_bypassed(), 0);
}

#[test]
fn bypass_ledger_is_kept_while_the_routed_leg_stays_down() {
    let h = ProxyHealth::unknown();
    h.note_bypass();
    let mut failures = 0;
    for _ in 0..DOWN_AFTER_CONSECUTIVE_FAILURES {
        record_probe_result(&h, &mut failures, false, "http://127.0.0.1:9989/api/health");
    }
    assert_eq!(h.get(), HealthState::Down);
    assert_eq!(h.take_bypassed(), 1);
}

#[test]
fn bypass_ledger_is_shared_by_clones() {
    let h = ProxyHealth::unknown();
    let h2 = h.clone();
    h.note_bypass();
    assert_eq!(h2.note_bypass(), 2);
}

#[test]
fn clone_shares_state() {
    let h = ProxyHealth::unknown();
    let h2 = h.clone();
    h.set(HealthState::Up);
    assert!(h2.is_up());
}

#[test]
fn body_status_forwarding_variants() {
    assert!(body_status_is_forwarding(
        br#"{"status":"ok","embedding":{}}"#
    ));
    assert!(body_status_is_forwarding(br#"{ "status" : "ok" }"#));
    assert!(body_status_is_forwarding(
        br#"{"embedding":{},"status":"ok"}"#
    ));
    assert!(body_status_is_forwarding(br#"{"status":"degraded"}"#));
    assert!(body_status_is_forwarding(
        br#"{"status":"degraded","llm":{"consecutive_failures":3}}"#
    ));
}

#[test]
fn body_status_not_forwarding_variants() {
    assert!(!body_status_is_forwarding(br#"{"status":"starting"}"#));
    assert!(!body_status_is_forwarding(br#"{"state":"ok"}"#));
    assert!(!body_status_is_forwarding(b"not json at all"));
    assert!(!body_status_is_forwarding(b""));
    assert!(!body_status_is_forwarding(&[0xff, 0xfe]));
    assert!(!body_status_is_forwarding(br#"{"status":true}"#));
    assert!(!body_status_is_forwarding(br#""ok""#));
}

#[test]
fn nested_status_does_not_shadow_top_level() {
    assert!(body_status_is_forwarding(
        br#"{"embedding":{"status":"degraded"},"status":"ok"}"#
    ));
    assert!(body_status_is_forwarding(
        br#"{"embedding":{"status":"ok"},"status":"degraded"}"#
    ));
    assert!(!body_status_is_forwarding(
        br#"{"embedding":{"status":"ok"},"status":"starting"}"#
    ));
}

#[tokio::test]
async fn routed_leg_connects_reflects_liveness() -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    assert!(
        routed_leg_connects(&addr.to_string(), Duration::from_millis(500)).await,
        "connect to a live routed leg is Up"
    );

    assert!(
        !routed_leg_connects("127.0.0.1:1", Duration::from_millis(500)).await,
        "connect to a dead routed leg is Down"
    );
    Ok(())
}

async fn raw_http_stub(raw_response: Vec<u8>) -> anyhow::Result<std::net::SocketAddr> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
            }
            let _ = stream.write_all(&raw_response).await;
            let _ = stream.shutdown().await;
        }
    });
    Ok(addr)
}

async fn live_routed_leg() -> anyhow::Result<std::net::SocketAddr> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move { while listener.accept().await.is_ok() {} });
    Ok(addr)
}

#[tokio::test]
async fn probe_once_reads_down_on_non_200_even_with_live_routed_leg() -> anyhow::Result<()> {
    let routed_addr = live_routed_leg().await?;
    let health_addr =
        raw_http_stub(b"HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\n\r\n".to_vec())
            .await?;
    let client: Client<HttpConnector, Empty<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let up = probe_once(
        &client,
        &format!("http://{health_addr}/api/health"),
        &routed_addr.to_string(),
        Duration::from_millis(500),
    )
    .await;

    assert!(
        !up,
        "a non-200 health response must read Down even though the routed leg connects"
    );
    Ok(())
}

#[tokio::test]
async fn probe_once_reads_up_on_degraded_status_with_live_routed_leg() -> anyhow::Result<()> {
    let routed_addr = live_routed_leg().await?;
    let body = br#"{"status":"degraded","llm":{"consecutive_failures":3}}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
        body.len(),
        std::str::from_utf8(body)?
    );
    let health_addr = raw_http_stub(response.into_bytes()).await?;
    let client: Client<HttpConnector, Empty<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let up = probe_once(
        &client,
        &format!("http://{health_addr}/api/health"),
        &routed_addr.to_string(),
        Duration::from_millis(500),
    )
    .await;

    assert!(
        up,
        "a 200 response with status \"degraded\" must read Up when the routed leg connects"
    );
    Ok(())
}

#[test]
fn host_header_extracted_from_url() {
    assert_eq!(
        host_header_for("http://127.0.0.1:9989/api/health"),
        "127.0.0.1:9989"
    );
    assert_eq!(host_header_for("http://localhost/x"), "localhost");
}

const URL: &str = "http://127.0.0.1:9989/api/health";

#[test]
fn one_failure_then_success_stays_up() {
    let h = ProxyHealth::unknown();
    let mut failures = 0;
    record_probe_result(&h, &mut failures, true, URL);
    record_probe_result(&h, &mut failures, false, URL);
    assert_eq!(h.get(), HealthState::Up);
    record_probe_result(&h, &mut failures, true, URL);
    assert_eq!(h.get(), HealthState::Up);
    assert_eq!(failures, 0);
}

#[test]
fn two_consecutive_failures_go_down() {
    let h = ProxyHealth::unknown();
    let mut failures = 0;
    record_probe_result(&h, &mut failures, true, URL);
    record_probe_result(&h, &mut failures, false, URL);
    record_probe_result(&h, &mut failures, false, URL);
    assert_eq!(h.get(), HealthState::Down);
}

#[test]
fn success_resets_the_failure_streak() {
    let h = ProxyHealth::unknown();
    let mut failures = 0;
    record_probe_result(&h, &mut failures, true, URL);
    record_probe_result(&h, &mut failures, false, URL);
    record_probe_result(&h, &mut failures, true, URL);
    record_probe_result(&h, &mut failures, false, URL);
    assert_eq!(h.get(), HealthState::Up);
}

#[test]
fn one_failure_from_unknown_stays_unknown() {
    let h = ProxyHealth::unknown();
    let mut failures = 0;
    record_probe_result(&h, &mut failures, false, URL);
    assert_eq!(h.get(), HealthState::Unknown);
}

#[test]
fn down_recovers_on_first_success() {
    let h = ProxyHealth::unknown();
    let mut failures = 0;
    record_probe_result(&h, &mut failures, false, URL);
    record_probe_result(&h, &mut failures, false, URL);
    record_probe_result(&h, &mut failures, true, URL);
    assert_eq!(h.get(), HealthState::Up);
}

#[test]
fn probe_timeout_is_raised_above_handler_worst_case() {
    assert!(MIN_PROBE_TIMEOUT > HANDLER_WORST_CASE);
    assert_eq!(
        effective_probe_timeout(Duration::from_secs(1)),
        MIN_PROBE_TIMEOUT
    );
    assert_eq!(
        effective_probe_timeout(Duration::from_secs(9)),
        Duration::from_secs(9)
    );
}
