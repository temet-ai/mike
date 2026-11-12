use std::time::Duration;

use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_bytes, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::raw_sse_mock;
use raw_sse_mock::MockSseEvent;

const CSE_ID: &str = "cse_01HRXsUQqSxtgFFa4Je2TGmP";

struct Harness {
    proxy_url: String,
    inspect_url: String,
    shutdown: CancellationToken,
}

impl Harness {
    async fn counter(&self, series: &str) -> anyhow::Result<f64> {
        let body = reqwest::get(format!("{}/api/metrics", self.inspect_url))
            .await?
            .text()
            .await?;
        let value = body
            .lines()
            .find_map(|line| line.strip_prefix(series))
            .and_then(|rest| rest.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(value)
    }
}

async fn start_proxy(upstream_url: &str) -> anyhow::Result<Harness> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
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
    let proxy_addr = proxy_listener.local_addr()?;
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

    Ok(Harness {
        proxy_url: format!("http://{proxy_addr}"),
        inspect_url: format!("http://{inspect_addr}"),
        shutdown,
    })
}

#[tokio::test]
async fn heartbeat_takes_the_capture_route() -> anyhow::Result<()> {
    let heartbeat = br#"{"session_id":"9f0c2c3a-1111-4b2b-8f6d-0a1b2c3d4e5f","worker_epoch":7,"idle_seconds":143,"current_interval_seconds":20}"#;
    let upstream_reply = r#"{"ok":true,"next_interval_seconds":20}"#;

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/code/sessions/{CSE_ID}/worker/heartbeat")))
        .and(body_bytes(heartbeat.to_vec()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(upstream_reply),
        )
        .expect(1)
        .mount(&upstream)
        .await;

    let harness = start_proxy(&upstream.uri()).await?;
    let response = reqwest::Client::new()
        .post(format!(
            "{}/v1/code/sessions/{CSE_ID}/worker/heartbeat",
            harness.proxy_url
        ))
        .header("content-type", "application/json")
        .body(heartbeat.to_vec())
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        response.text().await?,
        upstream_reply,
        "the response must reach the client exactly as upstream sent it"
    );

    assert_eq!(
        harness
            .counter(
                r#"proxy_remote_control_calls_total{endpoint="/worker/heartbeat",outcome="not_persisted"}"#
            )
            .await?,
        1.0,
        "the heartbeat must be counted as a Remote Control call taking the capture route"
    );
    assert_eq!(
        harness
            .counter(r#"proxy_sessionless_requests_total{reason="missing_session_header"}"#)
            .await?,
        0.0,
        "a captured Remote Control call must not also count as sessionless"
    );

    harness.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn a_non_matching_path_takes_the_sessionless_route() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string("{}"),
        )
        .expect(1)
        .mount(&upstream)
        .await;

    let harness = start_proxy(&upstream.uri()).await?;
    let response = reqwest::Client::new()
        .post(format!("{}/v1/messages", harness.proxy_url))
        .header("content-type", "application/json")
        .body(r#"{"model":"m"}"#)
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        harness
            .counter(r#"proxy_sessionless_requests_total{reason="missing_session_header"}"#)
            .await?,
        1.0,
        "a request with no session id and no cse_ id in its path is captured as sessionless"
    );

    let metrics = reqwest::get(format!("{}/api/metrics", harness.inspect_url))
        .await?
        .text()
        .await?;
    assert!(
        !metrics.contains(
            r#"proxy_remote_control_calls_total{endpoint="other",outcome="not_persisted"} 1"#
        ),
        "a /v1/messages request must not be mistaken for a Remote Control call; \
         /api/metrics body: {metrics}"
    );

    harness.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn a_session_path_without_a_cse_id_takes_the_sessionless_route() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/code/sessions/not-a-channel/worker"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&upstream)
        .await;

    let harness = start_proxy(&upstream.uri()).await?;
    let response = reqwest::get(format!(
        "{}/v1/code/sessions/not-a-channel/worker",
        harness.proxy_url
    ))
    .await?;

    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        harness
            .counter(r#"proxy_sessionless_requests_total{reason="missing_session_header"}"#)
            .await?,
        1.0,
        "the cse_ prefix is what makes a path a channel; without it there is no identity to key on"
    );

    harness.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn a_transcript_push_is_captured_and_forwarded_unaltered() -> anyhow::Result<()> {
    let transcript = br#"{"events":[{"type":"assistant_text","text":"a long transcript chunk"}]}"#;

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/code/sessions/{CSE_ID}/worker/events")))
        .and(body_bytes(transcript.to_vec()))
        .respond_with(ResponseTemplate::new(202).set_body_string(r#"{"accepted":true}"#))
        .expect(1)
        .mount(&upstream)
        .await;

    let harness = start_proxy(&upstream.uri()).await?;
    let response = reqwest::Client::new()
        .post(format!(
            "{}/v1/code/sessions/{CSE_ID}/worker/events",
            harness.proxy_url
        ))
        .header("content-type", "application/json")
        .body(transcript.to_vec())
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 202);
    assert_eq!(response.text().await?, r#"{"accepted":true}"#);
    assert_eq!(
        harness
            .counter(
                r#"proxy_remote_control_calls_total{endpoint="/worker/events",outcome="not_persisted"}"#
            )
            .await?,
        1.0,
        "a digest-only endpoint is still a captured call"
    );

    harness.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn the_inbound_stream_is_forwarded_as_it_arrives_and_its_lifecycle_recorded()
-> anyhow::Result<()> {
    let mut second = MockSseEvent::new("message", r#"{"reply":"from the phone"}"#);
    second.delay = Duration::from_secs(1);
    let upstream_addr = raw_sse_mock::raw_sse_mock(
        vec![MockSseEvent::new("message", r#"{"reply":"first"}"#), second],
        None,
    )
    .await?;

    let harness = start_proxy(&format!("http://{upstream_addr}")).await?;
    let mut response = reqwest::Client::new()
        .get(format!(
            "{}/v1/code/sessions/{CSE_ID}/worker/events/stream",
            harness.proxy_url
        ))
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream"),
        "the stream's content type must survive the proxy untouched"
    );

    let first_chunk = tokio::time::timeout(Duration::from_millis(500), response.chunk())
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "the first event did not reach the client within 500ms, well before the \
                 upstream sent its second event at 1s — the response is being buffered"
            )
        })??
        .ok_or_else(|| anyhow::anyhow!("the stream closed before sending anything"))?;
    assert!(
        String::from_utf8_lossy(&first_chunk).contains(r#"{"reply":"first"}"#),
        "the first event must arrive verbatim; got {:?}",
        String::from_utf8_lossy(&first_chunk)
    );

    assert_eq!(
        harness
            .counter(r#"proxy_remote_control_stream_events_total{event="opened"}"#)
            .await?,
        1.0,
        "opening the stream must be recorded as it opens, not when it ends"
    );

    while tokio::time::timeout(Duration::from_secs(5), response.chunk())
        .await?
        .transpose()
        .is_some()
    {}

    let closes = poll_until(Duration::from_secs(5), || async {
        let eof = harness
            .counter(r#"proxy_remote_control_stream_events_total{event="closed_eof"}"#)
            .await?;
        Ok(eof)
    })
    .await?;
    assert_eq!(
        closes, 1.0,
        "the close of the stream is the signal worth having — a stream that keeps \
         reconnecting looks healthy from outside without it"
    );

    harness.shutdown.cancel();
    Ok(())
}

async fn poll_until<F, Fut>(limit: Duration, mut probe: F) -> anyhow::Result<f64>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<f64>>,
{
    let deadline = tokio::time::Instant::now() + limit;
    let mut last = 0.0;
    while tokio::time::Instant::now() < deadline {
        last = probe().await?;
        if last > 0.0 {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    anyhow::bail!("counter never left zero within {limit:?}; last read {last}")
}
