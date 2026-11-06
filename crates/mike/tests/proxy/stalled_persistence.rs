use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use mike::api::handlers::{capture_outage_notices, refresh_capture_outage_now};
use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use mike::store::test_support::TestDb;
use serde_json::Value;
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn unreachable_database_still_forwards_and_notifies_once() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let upstream = MockServer::start().await;
    let response_body = MockResponse::new("msg_stalled").text("still here").build();
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&response_body)
                .insert_header("content-type", "application/json"),
        )
        .expect(1)
        .mount(&upstream)
        .await;

    let notices = capture_outage_notices();
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream.uri().as_str(),
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        test_db.database_url.as_str(),
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_url = format!("http://{}", proxy_listener.local_addr()?);
    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);
    let proxy_app = proxy_router(state.clone());
    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });

    test_db.cleanup().await?;

    for _ in 0..3 {
        refresh_capture_outage_now(&state).await;
    }

    let resp = reqwest::Client::new()
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&RequestBody::default().content("hi").max_tokens(8).build())
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "a request must still be forwarded while the database is unreachable"
    );
    let body: Value = resp.json().await?;
    assert_eq!(
        body, response_body,
        "the upstream body is returned unchanged"
    );

    let sent: Vec<String> = notices.try_iter().collect();
    assert_eq!(
        sent.len(),
        1,
        "three failed readings notify exactly once: {sent:?}"
    );
    assert!(
        sent.iter().all(|m| m.contains("cannot save captures")),
        "{sent:?}"
    );

    shutdown.cancel();
    Ok(())
}
