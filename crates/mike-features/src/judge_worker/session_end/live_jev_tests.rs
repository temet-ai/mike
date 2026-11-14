use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::LivePoll;
use crate::analysis::EmbeddingService;
use crate::analysis::embeddings::EmbeddingConfig;
use crate::judge_worker::ownership_test_support::Fixture;
use crate::judge_worker::session_end::run::SessionJudgementV3;
use crate::judge_worker::session_end::schema::SessionVerdictV3;
use crate::llm::OpenRouterClient;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionMode;
use crate::settings::SettingsOverlay;

fn judged_by_model() -> SessionJudgementV3 {
    SessionJudgementV3 {
        verdict: SessionVerdictV3 {
            outcome: "partial".to_owned(),
            quality_score: 0.4,
            rationale: "the model's reading".to_owned(),
            overview: "overview text".to_owned(),
            topics: Vec::new(),
            learnings: Vec::new(),
            end_label: "done".to_owned(),
            end_evidence: String::new(),
            end_confidence: "high".to_owned(),
            failure: String::new(),
            failure_tags: Vec::new(),
            severity: "none".to_owned(),
        },
        facts: None,
        rule_fired: None,
        end: None,
        input_text: "the end of the session".to_owned(),
        digest_version: "v1".to_owned(),
        as_of: chrono::Utc::now(),
        usage: crate::llm::CallUsage::default(),
        quality_unknown: false,
    }
}

fn answered() -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {
            "outcome": {"type": "choice", "choice": "success",
                "probabilities": {"success": 0.9, "partial": 0.05, "failure": 0.03, "unknown": 0.02}},
            "quality": {"type": "score", "score": 4.0,
                "probabilities": {"0": 0.0, "1": 0.0, "2": 0.0, "3": 0.0, "4": 1.0}}
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

fn decider(server: &MockServer, f: &Fixture) -> anyhow::Result<Decider> {
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Decider::with_mode(
        &client,
        &f.persist,
        &SettingsOverlay::default(),
        crate::judge_worker::session_poll::jev::POINT,
        DecisionMode::On,
    )
    .ok_or_else(|| anyhow::anyhow!("no decider"))
}

fn poll<'a>(
    f: &'a Fixture,
    embedding: &'a EmbeddingService,
    jev: Option<&'a Decider>,
) -> LivePoll<'a> {
    LivePoll {
        persist: &f.persist,
        llm: &f.llm,
        embedding,
        bundle_id: f.bundle.id,
        lookback_hours: 24,
        max_attempts_per_day: 5,
        batch: 10,
        turn_judge_live_bundle_id: f.bundle.id,
        turn_confidence_threshold: 0.5,
        jev,
    }
}

#[tokio::test]
async fn live_poll_with_the_point_off_keeps_the_models_outcome() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let embedding = EmbeddingService::new(&EmbeddingConfig::default());
    let mut judged = judged_by_model();
    let decided = poll(&f, &embedding, None).apply_jev(1, &mut judged).await;
    assert!(decided.is_none());
    assert_eq!(judged.verdict.outcome, "partial");
    assert!((judged.verdict.quality_score - 0.4).abs() < 1e-9);
    Ok(())
}

#[tokio::test]
async fn live_poll_with_the_point_on_takes_outcome_and_quality_from_jev() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let embedding = EmbeddingService::new(&EmbeddingConfig::default());
    let server = stub(200, answered()).await;
    let decider = decider(&server, &f)?;
    let mut judged = judged_by_model();
    let decided = poll(&f, &embedding, Some(&decider))
        .apply_jev(1, &mut judged)
        .await;
    assert!(decided.is_some());
    assert_eq!(judged.verdict.outcome, "success");
    assert!((judged.verdict.quality_score - 1.0).abs() < 1e-9);
    assert_eq!(judged.verdict.overview, "overview text");
    Ok(())
}

#[tokio::test]
async fn live_poll_with_jev_failing_keeps_the_models_outcome() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let embedding = EmbeddingService::new(&EmbeddingConfig::default());
    let server = stub(500, json!({"error": "down"})).await;
    let decider = decider(&server, &f)?;
    let mut judged = judged_by_model();
    let decided = poll(&f, &embedding, Some(&decider))
        .apply_jev(1, &mut judged)
        .await;
    assert!(decided.is_none());
    assert_eq!(judged.verdict.outcome, "partial");
    assert!((judged.verdict.quality_score - 0.4).abs() < 1e-9);
    Ok(())
}
