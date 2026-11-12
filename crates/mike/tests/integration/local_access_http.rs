use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use futures::StreamExt;
use mike::app::{inspect_router_on, proxy_router, spa_router_on};
use mike::config::ProxyConfig;
use mike::state::AppState;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::{Error, client::IntoClientRequest};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CAPTURE: &str = "synthetic-local-access-secret";
const REQUEST: &str = "{\"model\":\"claude-test\",\"stream\":true,\"max_tokens\":32,\"messages\":[{\"role\":\"user\",\"content\":\"synthetic-local-access-secret\"}]}";

struct Harness {
    addresses: [SocketAddr; 2],
    proxy: SocketAddr,
    shutdown: CancellationToken,
    upstream: MockServer,
    response: String,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn serve(listener: TcpListener, app: Router, shutdown: CancellationToken) {
    let _ = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await;
}

fn synthetic_response() -> String {
    let events = [
        (
            "message_start",
            serde_json::json!({"type":"message_start","message":{"id":"msg_local_access","type":"message","role":"assistant","content":[],"model":"claude-test","usage":{"input_tokens":1,"output_tokens":0}}}),
        ),
        (
            "content_block_start",
            serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        ),
        (
            "content_block_delta",
            serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":CAPTURE}}),
        ),
        (
            "content_block_stop",
            serde_json::json!({"type":"content_block_stop","index":0}),
        ),
        (
            "message_delta",
            serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}),
        ),
        ("message_stop", serde_json::json!({"type":"message_stop"})),
    ];
    events
        .into_iter()
        .map(|(name, data)| format!("event: {name}\ndata: {data}\n\n"))
        .collect()
}

async fn start() -> anyhow::Result<Harness> {
    let upstream = MockServer::start().await;
    let response = synthetic_response();
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(response.as_bytes(), "text/event-stream"),
        )
        .mount(&upstream)
        .await;
    let mut config = ProxyConfig::for_tests_in_memory();
    config.upstream_url = upstream.uri();
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();
    let inspect = TcpListener::bind("127.0.0.1:0").await?;
    let spa = TcpListener::bind("127.0.0.1:0").await?;
    let proxy = TcpListener::bind("127.0.0.1:0").await?;
    let addresses = [inspect.local_addr()?, spa.local_addr()?];
    let proxy_addr = proxy.local_addr()?;
    tokio::spawn(serve(
        inspect,
        inspect_router_on(state.clone(), addresses[0]),
        shutdown.clone(),
    ));
    tokio::spawn(serve(
        spa,
        spa_router_on(state.clone(), addresses[1]),
        shutdown.clone(),
    ));
    tokio::spawn(serve(proxy, proxy_router(state), shutdown.clone()));
    Ok(Harness {
        addresses,
        proxy: proxy_addr,
        shutdown,
        upstream,
        response,
    })
}

#[tokio::test]
async fn both_real_listeners_reject_untrusted_reads_mutations_and_upgrades() -> anyhow::Result<()> {
    let harness = start().await?;
    let client = reqwest::Client::new();
    for address in harness.addresses {
        let url = format!("http://{address}");
        assert_eq!(
            client
                .get(format!("{url}/api/requests"))
                .send()
                .await?
                .status(),
            200
        );
        for origin in ["http://unrelated.example", "null", "https://127.0.0.1:9989"] {
            for (method, path) in [
                (reqwest::Method::GET, "/api/requests"),
                (reqwest::Method::PATCH, "/api/settings/features/composition"),
            ] {
                let response = client
                    .request(method, format!("{url}{path}"))
                    .header("origin", origin)
                    .header("content-type", "text/plain")
                    .body("{\"enabled\":true}")
                    .send()
                    .await?;
                assert_eq!(response.status(), 403, "{address}: {origin}");
                assert!(!response.text().await?.contains(CAPTURE));
            }
            let mut upgrade = format!("ws://{address}/api/ws").into_client_request()?;
            upgrade.headers_mut().insert("origin", origin.parse()?);
            match connect_async(upgrade).await {
                Err(Error::Http(response)) => {
                    assert_eq!(response.status(), 403);
                    assert!(
                        !String::from_utf8_lossy(response.body().as_deref().unwrap_or_default())
                            .contains(CAPTURE)
                    );
                }
                other => anyhow::bail!("untrusted upgrade was not forbidden: {other:?}"),
            }
        }
        let response = client
            .get(format!("{url}/"))
            .header("origin", "http://unrelated.example")
            .send()
            .await?;
        if address == harness.addresses[1] {
            assert_eq!(response.status(), 403);
        }
        let response = client
            .get(format!("{url}/api/requests"))
            .header("host", "evil.example:9989")
            .send()
            .await?;
        assert_eq!(response.status(), 403);
        let response = client
            .get(format!("{url}/api/requests"))
            .header("origin", &url)
            .send()
            .await?;
        assert_eq!(response.status(), 200);
    }
    Ok(())
}

#[tokio::test]
async fn native_websockets_receive_capture_on_both_ports_and_proxy_bytes_are_unchanged()
-> anyhow::Result<()> {
    let harness = start().await?;
    let (mut inspect, _) = connect_async(format!("ws://{}/api/ws", harness.addresses[0])).await?;
    let (mut spa, _) = connect_async(format!("ws://{}/api/ws", harness.addresses[1])).await?;
    let response = reqwest::Client::new()
        .post(format!("http://{}/v1/messages", harness.proxy))
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "7b83aba3-ea81-4550-9b92-ad5939192081",
        )
        .body(REQUEST)
        .send()
        .await?;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.bytes().await?.as_ref(),
        harness.response.as_bytes()
    );
    let requests = harness
        .upstream
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("missing mock requests"))?;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, REQUEST.as_bytes());
    for socket in [&mut inspect, &mut spa] {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let frame = socket
                    .next()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("socket closed"))??;
                if frame.to_text().is_ok_and(|text| text.contains(CAPTURE)) {
                    break;
                }
            }
            anyhow::Ok(())
        })
        .await??;
        socket.close(None).await?;
    }
    Ok(())
}
