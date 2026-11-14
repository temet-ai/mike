use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::test_support::DecisionsFixture;
use super::*;
use crate::feature_toggles::FeatureToggles;
use crate::llm::OpenRouterClient;
use crate::settings::SettingsOverlay;
use crate::store::persistence::fleet::tests::seed::{seed_request, seed_response, usage};
use crate::store::persistence::launch_repo::LaunchPersist as _;

struct Seeded {
    first: i64,
    second: i64,
}

async fn seed(fx: &DecisionsFixture) -> anyhow::Result<Seeded> {
    let Some((session_id, project_id)) = fx.handle.launch().resolve_session_core(fx.worker).await?
    else {
        anyhow::bail!("the worker session was not seeded");
    };
    let pool = fx.handle.pool();
    let now = Utc::now();
    let first = seed_request(
        pool,
        session_id,
        project_id,
        None,
        now + Duration::seconds(2),
    )
    .await?;
    let second = seed_request(
        pool,
        session_id,
        project_id,
        None,
        now + Duration::seconds(3),
    )
    .await?;
    sqlx::query!(
        "UPDATE requests.requests SET human_prompt = 'Stop hook feedback: keep going' WHERE id = $1",
        first
    )
    .execute(pool)
    .await?;
    sqlx::query!(
        "UPDATE requests.requests SET human_prompt = 'please start', author_kind_id = 1 WHERE id = $1",
        second
    )
    .execute(pool)
    .await?;
    seed_response(pool, first, &usage(100, 0, 0)).await?;
    seed_response(pool, second, &usage(200, 0, 0)).await?;
    Ok(Seeded { first, second })
}

fn labelled(second: i64) -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {format!("step_{second}"): {"type": "choice", "choice": "work",
            "probabilities": {"work": 0.9, "machinery": 0.05, "waiting": 0.05}}},
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

#[tokio::test]
async fn effort_the_trigger_labels_hooks_and_jev_labels_the_rest_and_shares_sum_by_tokens()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let seeded = seed(&fx).await?;
    assert_eq!(observe(&fx.handle, &fx.run).await, 2);
    let before = document(&fx.handle, &fx.run).await;
    assert_eq!(before["all_seats"]["labels"]["unlabelled"]["tokens"], 207);
    let server = stub(200, labelled(seeded.second)).await;
    let found = decisions(&server, &fx)?;
    let done = label_pass(Some(&found), Mode::On, &fx.handle, (&fx.run, fx.fleet_id)).await;
    assert_eq!(done, 1);
    let after = document(&fx.handle, &fx.run).await;
    assert_eq!(after["all_seats"]["tokens"], 314);
    assert_eq!(after["all_seats"]["labels"]["machinery"]["tokens"], 107);
    assert_eq!(after["all_seats"]["labels"]["work"]["share_percent"], 65.9);
    let _ = seeded.first;
    fx.finish().await
}

#[tokio::test]
async fn effort_a_failing_client_leaves_the_content_unlabelled_and_pauses() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    seed(&fx).await?;
    observe(&fx.handle, &fx.run).await;
    let server = stub(500, json!({})).await;
    let found = decisions(&server, &fx)?;
    let run = (fx.run.as_str(), fx.fleet_id);
    assert_eq!(label_pass(Some(&found), Mode::On, &fx.handle, run).await, 0);
    assert_eq!(label_pass(Some(&found), Mode::On, &fx.handle, run).await, 0);
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        1
    );
    let doc = document(&fx.handle, &fx.run).await;
    assert_eq!(doc["all_seats"]["labels"]["unlabelled"]["requests"], 1);
    fx.finish().await
}

#[tokio::test]
async fn effort_with_the_point_off_records_triggers_and_asks_nothing() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    seed(&fx).await?;
    assert_eq!(observe(&fx.handle, &fx.run).await, 2);
    let server = stub(200, json!({})).await;
    let found = decisions(&server, &fx)?;
    let run = (fx.run.as_str(), fx.fleet_id);
    assert_eq!(
        label_pass(Some(&found), Mode::Off, &fx.handle, run).await,
        0
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    fx.finish().await
}
