use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::test_support::DecisionsFixture;
use super::*;
use crate::feature_toggles::FeatureToggles;
use crate::llm::OpenRouterClient;
use crate::settings::SettingsOverlay;

fn answers(claims: f64, waiting: f64, unanswered: f64) -> Value {
    let noul = |value: f64| json!({"type": "noul", "noul": value});
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {
            "final_message_claims_done": noul(claims),
            "waiting_on_something_running": noul(waiting),
            "question_to_coordinator_unanswered": noul(unanswered),
        },
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

async fn stub(status: u16, body: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn layer(
    server: &MockServer,
    fx: &DecisionsFixture,
    on: bool,
) -> anyhow::Result<(FleetDecisions, Arc<ArcSwap<FeatureToggles>>)> {
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    let toggles = Arc::new(ArcSwap::from_pointee(FeatureToggles {
        decisions: on,
        ..FeatureToggles::default()
    }));
    let decisions = FleetDecisions::new(
        client,
        fx.handle.clone(),
        toggles.clone(),
        Arc::new(ArcSwap::from_pointee(SettingsOverlay::default())),
    );
    Ok((decisions, toggles))
}

fn facts(uncommitted: usize) -> StopFacts {
    StopFacts {
        seat: "worker-T1".into(),
        task: "T1".into(),
        uncommitted: (0..uncommitted).map(|i| format!("src/f{i}.rs")).collect(),
        last_change_at: None,
        check_passed_after_change: false,
        last_check: None,
        open_asks: Vec::new(),
        check_running: false,
        done_when: None,
    }
}

fn said_done() -> StopRequest {
    StopRequest {
        final_message: Some("T1 is finished and reported.".into()),
        stop_hook_active: false,
        probe: false,
    }
}

async fn decide(
    decisions: &FleetDecisions,
    mode: Mode,
    (facts, request): (StopFacts, StopRequest),
    fx: &DecisionsFixture,
) -> Decision {
    answer(
        Some(decisions),
        mode,
        (facts, request),
        (fx.worker, fx.fleet_id),
    )
    .await
    .decision
}

#[tokio::test]
async fn answer_a_done_claim_with_nothing_uncommitted_lets_the_stop_through() -> anyhow::Result<()>
{
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, answers(0.95, 0.05, 0.05)).await;
    let (decisions, _) = layer(&server, &fx, true)?;
    let got = decide(&decisions, Mode::On, (facts(0), said_done()), &fx).await;
    assert_eq!(got, Decision::LetThrough);
    fx.finish().await
}

#[tokio::test]
async fn answer_flipping_the_toggle_changes_the_next_stop_with_no_restart() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, answers(0.05, 0.05, 0.05)).await;
    let (decisions, toggles) = layer(&server, &fx, true)?;
    assert_eq!(
        decide(&decisions, Mode::On, (facts(0), said_done()), &fx).await,
        Decision::PushBack
    );
    toggles.store(Arc::new(FeatureToggles {
        decisions: false,
        ..FeatureToggles::default()
    }));
    assert_eq!(
        decide(&decisions, Mode::On, (facts(0), said_done()), &fx).await,
        Decision::Off
    );
    toggles.store(Arc::new(FeatureToggles {
        decisions: true,
        ..FeatureToggles::default()
    }));
    assert_eq!(
        decide(&decisions, Mode::On, (facts(0), said_done()), &fx).await,
        Decision::PushBack
    );
    fx.finish().await
}

#[tokio::test]
async fn answer_a_failing_client_is_the_off_path_and_never_a_let_through() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    for server in [
        stub(500, json!({})).await,
        stub(200, json!({"answers": 3})).await,
    ] {
        let (decisions, _) = layer(&server, &fx, true)?;
        let got = decide(&decisions, Mode::On, (facts(0), said_done()), &fx).await;
        assert_eq!(got, Decision::Off);
    }
    fx.finish().await
}

#[tokio::test]
async fn answer_uncommitted_unchecked_changes_are_pushed_back_even_when_the_client_fails()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(500, json!({})).await;
    let (decisions, _) = layer(&server, &fx, true)?;
    let got = decide(&decisions, Mode::On, (facts(3), said_done()), &fx).await;
    assert_eq!(got, Decision::PushBack);
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    fx.finish().await
}

#[tokio::test]
async fn answer_shadow_and_off_modes_do_not_decide() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, answers(0.05, 0.05, 0.05)).await;
    let (decisions, _) = layer(&server, &fx, true)?;
    for mode in [Mode::Shadow, Mode::Off] {
        let got = decide(&decisions, mode, (facts(3), said_done()), &fx).await;
        assert_eq!(got, Decision::Off, "{mode:?}");
    }
    let _ = Uuid::nil();
    fx.finish().await
}
