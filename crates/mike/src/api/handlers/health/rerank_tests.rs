use chrono::Utc;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::rerank::describe;
use super::*;
use crate::analysis::rerank::{RerankPhase, RerankStatus};
use crate::semantic::egress::EgressState;

fn status(phase: RerankPhase, url: &str) -> RerankStatus {
    RerankStatus {
        phase,
        url: url.to_owned(),
        model: "bge-reranker-v2-m3".to_owned(),
        breaker_open: false,
        last_ok_at: None,
        last_error: None,
        last_error_at: None,
    }
}

#[test]
fn describe_when_disabled_is_off_and_names_the_settings_page() {
    let health = describe(status(RerankPhase::Off, "http://127.0.0.1:11435"), false);

    assert_eq!(health.state, RerankRowState::Off);
    assert!(!health.enabled);
    assert!(
        health.fix.is_some_and(
            |fix| fix.contains("rerank.enabled") && fix.contains("/settings/embedding")
        )
    );
}

#[test]
fn describe_when_ready_is_ok_without_a_remedy() {
    let health = describe(status(RerankPhase::Ready, "http://127.0.0.1:11435"), false);

    assert_eq!(health.state, RerankRowState::Ok);
    assert!(health.enabled);
    assert!(health.fix.is_none());
}

#[test]
fn describe_when_idle_or_loading_is_ok() {
    for phase in [RerankPhase::Idle, RerankPhase::Loading] {
        let health = describe(status(phase, "http://127.0.0.1:11435"), false);
        assert_eq!(health.state, RerankRowState::Ok, "{phase:?}");
        assert!(health.fix.is_none());
    }
}

#[test]
fn describe_when_unavailable_is_degraded_with_the_error_and_a_remedy() {
    let mut unavailable = status(RerankPhase::Unavailable, "http://127.0.0.1:11435");
    unavailable.last_error = Some("connection refused".to_owned());
    unavailable.last_error_at = Some(Utc::now());
    unavailable.breaker_open = true;

    let health = describe(unavailable, false);

    assert_eq!(health.state, RerankRowState::Degraded);
    assert!(
        health.detail.contains("connection refused"),
        "{}",
        health.detail
    );
    assert!(health.detail.contains("paused"), "{}", health.detail);
    assert!(health.last_error_at.is_some());
    assert!(
        health
            .fix
            .is_some_and(|fix| fix.contains("http://127.0.0.1:11435"))
    );
}

#[test]
fn describe_when_remote_and_egress_not_allowed_is_inert_and_names_the_permission() {
    let health = describe(
        status(RerankPhase::Ready, "http://192.168.1.20:11435"),
        false,
    );

    assert_eq!(health.state, RerankRowState::Inert);
    assert_eq!(health.egress, EgressState::RemoteBlocked);
    assert!(
        health
            .fix
            .is_some_and(|fix| fix.contains("semantic.allow_remote_embedding"))
    );
}

#[test]
fn describe_when_remote_and_egress_allowed_follows_the_server_state() {
    let health = describe(
        status(RerankPhase::Ready, "http://192.168.1.20:11435"),
        true,
    );

    assert_eq!(health.state, RerankRowState::Ok);
    assert_eq!(health.egress, EgressState::RemoteAllowed);
}

#[tokio::test]
async fn rerank_health_follows_real_call_outcomes_on_the_app_state() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    assert_eq!(rerank_health(&state).state, RerankRowState::Off);

    let service = state.rerank_service();
    service.apply_setting("rerank.enabled", &json!(true));
    service.apply_setting("rerank.url", &json!(server.uri()));
    assert_eq!(rerank_health(&state).phase, RerankPhase::Idle);

    service.health().await;
    let ready = rerank_health(&state);
    assert_eq!(ready.state, RerankRowState::Ok);
    assert_eq!(ready.phase, RerankPhase::Ready);
    assert!(ready.last_ok_at.is_some());

    service.apply_setting("rerank.url", &json!("http://127.0.0.1:1"));
    service.health().await;
    let down = rerank_health(&state);
    assert_eq!(down.state, RerankRowState::Degraded);
    assert!(down.fix.is_some());
    Ok(())
}

#[tokio::test]
async fn health_response_carries_a_rerank_row_without_changing_overall_status() -> anyhow::Result<()>
{
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let Json(before) = health(State(state.clone())).await;
    let before = serde_json::to_value(&before)?;
    assert_eq!(before["rerank"]["state"], "off");
    state
        .rerank_service()
        .apply_setting("rerank.enabled", &json!(true));
    state
        .rerank_service()
        .apply_setting("rerank.url", &json!("http://127.0.0.1:1"));
    state.rerank_service().health().await;

    let Json(response) = health(State(state)).await;
    let body = serde_json::to_value(&response)?;

    assert_eq!(body["rerank"]["state"], "degraded");
    assert_eq!(body["rerank"]["enabled"], true);
    assert!(body["rerank"]["fix"].is_string());
    assert_eq!(body["status"], before["status"]);
    Ok(())
}
