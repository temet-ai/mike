use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::service_tests::enabled;
use super::warmup::{WARMUP_WINDOW, WarmGate, run};
use super::{RerankConfig, RerankService};
use crate::store::models::{ProxyEvent, RowChangedOp, RowChangedTable};

fn row_changed(table: RowChangedTable, op: RowChangedOp) -> ProxyEvent {
    ProxyEvent::RowChanged {
        table,
        id: "1".into(),
        session_id: None,
        op,
    }
}

async fn health_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

async fn deliver(
    service: RerankService,
    window: Duration,
    events: Vec<ProxyEvent>,
) -> anyhow::Result<()> {
    let (sender, receiver) = broadcast::channel(32);
    for event in events {
        sender.send(event).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    drop(sender);
    run(receiver, service, window, CancellationToken::new()).await;
    Ok(())
}

async fn health_calls(server: &MockServer) -> anyhow::Result<usize> {
    let requests = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("request recording is off"))?;
    Ok(requests
        .iter()
        .filter(|r| r.url.path() == "/health")
        .count())
}

fn new_session() -> ProxyEvent {
    row_changed(RowChangedTable::Sessions, RowChangedOp::Insert)
}

#[test]
fn warm_gate_after_a_permit_refuses_until_the_window_has_passed() {
    let start = Instant::now();
    let mut gate = WarmGate::new(WARMUP_WINDOW);

    assert!(gate.permit(start));
    assert!(!gate.permit(start + Duration::from_secs(59)));
    assert!(gate.permit(start + WARMUP_WINDOW));
    assert!(!gate.permit(start + WARMUP_WINDOW + Duration::from_secs(1)));
}

#[tokio::test]
async fn warm_up_with_several_new_sessions_in_one_window_asks_for_health_once() -> anyhow::Result<()>
{
    let server = health_server().await;
    let service = RerankService::new(&enabled(&server.uri()));

    deliver(
        service,
        WARMUP_WINDOW,
        vec![new_session(), new_session(), new_session()],
    )
    .await?;

    assert_eq!(health_calls(&server).await?, 1);
    Ok(())
}

#[tokio::test]
async fn warm_up_when_the_window_has_passed_asks_for_health_again() -> anyhow::Result<()> {
    let server = health_server().await;
    let service = RerankService::new(&enabled(&server.uri()));

    deliver(service, Duration::ZERO, vec![new_session(), new_session()]).await?;

    assert_eq!(health_calls(&server).await?, 2);
    Ok(())
}

#[tokio::test]
async fn warm_up_when_rerank_is_disabled_makes_no_call() -> anyhow::Result<()> {
    let server = health_server().await;
    let service = RerankService::new(&RerankConfig {
        enabled: false,
        url: server.uri(),
        ..RerankConfig::default()
    });

    deliver(service, Duration::ZERO, vec![new_session()]).await?;

    assert_eq!(health_calls(&server).await?, 0);
    Ok(())
}

#[tokio::test]
async fn warm_up_ignores_events_that_are_not_a_new_session() -> anyhow::Result<()> {
    let server = health_server().await;
    let service = RerankService::new(&enabled(&server.uri()));

    deliver(
        service,
        Duration::ZERO,
        vec![
            row_changed(RowChangedTable::Sessions, RowChangedOp::Update),
            row_changed(RowChangedTable::Requests, RowChangedOp::Insert),
            row_changed(RowChangedTable::Sessions, RowChangedOp::Delete),
        ],
    )
    .await?;

    assert_eq!(health_calls(&server).await?, 0);
    Ok(())
}

#[tokio::test]
async fn warm_up_after_rerank_is_switched_on_uses_the_window_from_then() -> anyhow::Result<()> {
    let server = health_server().await;
    let service = RerankService::new(&RerankConfig {
        enabled: false,
        url: server.uri(),
        ..RerankConfig::default()
    });
    assert!(service.apply_setting("rerank.enabled", &serde_json::json!(true)));

    deliver(service, WARMUP_WINDOW, vec![new_session(), new_session()]).await?;

    assert_eq!(health_calls(&server).await?, 1);
    Ok(())
}
