use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::test_support::DecisionsFixture;
use super::*;
use crate::feature_toggles::FeatureToggles;
use crate::llm::OpenRouterClient;
use crate::settings::SettingsOverlay;

fn picked(key: &str) -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {"next_step": {"type": "choice", "choice": key,
            "probabilities": {"retry": 0.1, "fix_code": 0.1, "fix_env": 0.1, key: 0.8}}},
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

fn decisions(server: &MockServer, fx: &DecisionsFixture) -> anyhow::Result<FleetDecisions> {
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Ok(FleetDecisions::new(
        client,
        fx.handle.clone(),
        Arc::new(ArcSwap::from_pointee(FeatureToggles {
            decisions: true,
            ..FeatureToggles::default()
        })),
        Arc::new(ArcSwap::from_pointee(SettingsOverlay::default())),
    ))
}

async fn failed_check(
    fx: &DecisionsFixture,
    state: &str,
    exit: Option<i32>,
    output: &str,
) -> anyhow::Result<TreeCheckRow> {
    let id = fx
        .handle
        .fleet()
        .begin_tree_check(&fx.run, state, "make check", "worker-T1")
        .await?;
    fx.handle
        .fleet()
        .finish_tree_check(id, exit, output, Some(state))
        .await?;
    fx.handle
        .fleet()
        .latest_tree_check(&fx.run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the receipt is gone"))
}

#[tokio::test]
async fn store_an_unmatched_failure_gets_jevs_choice_in_the_receipt_line() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, picked("retry")).await;
    let found = decisions(&server, &fx)?;
    let row = failed_check(&fx, "s1", Some(1), "assertion failed: left == right").await?;
    store(&fx.handle, Some(&found), Mode::On, &row).await;
    let line = fx.handle.fleet().tree_check_triage(row.id).await?;
    assert!(line.is_some_and(|l| l.starts_with("triage (jev): retry")));
    fx.finish().await
}

#[tokio::test]
async fn store_a_failing_client_stores_no_line_beyond_the_rules() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(500, json!({})).await;
    let found = decisions(&server, &fx)?;
    let row = failed_check(&fx, "s2", Some(1), "assertion failed: left == right").await?;
    store(&fx.handle, Some(&found), Mode::On, &row).await;
    assert_eq!(fx.handle.fleet().tree_check_triage(row.id).await?, None);
    fx.finish().await
}

#[tokio::test]
async fn store_a_timeout_is_triaged_by_rule_with_the_layer_absent() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let row = failed_check(&fx, "s3", None, "").await?;
    store(&fx.handle, None, Mode::On, &row).await;
    let line = fx.handle.fleet().tree_check_triage(row.id).await?;
    assert!(line.is_some_and(|l| l.contains("fix the environment")));
    fx.finish().await
}

#[tokio::test]
async fn store_the_point_off_asks_nothing_for_an_unmatched_failure() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, picked("retry")).await;
    let found = decisions(&server, &fx)?;
    let row = failed_check(&fx, "s4", Some(1), "assertion failed").await?;
    store(&fx.handle, Some(&found), Mode::Off, &row).await;
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    assert_eq!(fx.handle.fleet().tree_check_triage(row.id).await?, None);
    fx.finish().await
}
