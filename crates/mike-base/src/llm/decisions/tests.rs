use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::openrouter::test_support::{TestSink, client_for, client_with_limiter};
use crate::llm::{LlmCallOutcome, LlmRole};

fn noul(text: &str) -> Question {
    Question::Noul {
        instructions: text.to_owned(),
        criteria: NoulCriteria {
            yes: "yes".to_owned(),
            no: "no".to_owned(),
        },
    }
}

fn request() -> DecisionRequest {
    let mut questions = BTreeMap::new();
    questions.insert("disclosed".to_owned(), noul("Was it disclosed?"));
    questions.insert("matters".to_owned(), noul("Does it matter?"));
    DecisionRequest {
        model: "typesafe/jev-1.14".to_owned(),
        state: json!({"final_message": "done"}),
        questions,
    }
}

fn context() -> record::DecisionContext {
    record::DecisionContext::new("unit_test", record::DecisionMode::Cli, "unit_test_v1")
}

fn answered(disclosed: serde_json::Value) -> serde_json::Value {
    json!({
        "id": "gen-dec-1",
        "model": "typesafe/jev-1.14-20261101",
        "provider": "TypeSafe",
        "answers": {
            "disclosed": disclosed,
            "matters": {"type": "noul", "noul": 0.2}
        },
        "usage": {"cost": 0.0000144, "input_tokens": 344, "output_tokens": 12}
    })
}

async fn served(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

#[test]
fn question_serialises_in_the_wire_shape() {
    let value = serde_json::to_value(noul("q")).unwrap_or_default();
    assert_eq!(
        value,
        json!({"type": "noul", "instructions": "q", "criteria": {"true": "yes", "false": "no"}})
    );
}

#[test]
fn decisions_url_replaces_the_v1_suffix() {
    assert_eq!(
        decisions_url("https://openrouter.ai/api/v1"),
        "https://openrouter.ai/api/alpha/decisions"
    );
    assert_eq!(
        decisions_url("http://127.0.0.1:9/"),
        "http://127.0.0.1:9/alpha/decisions"
    );
}

#[test]
fn parse_response_reads_typed_answers_and_refusals() {
    let body = answered(json!({"type": "refusal"})).to_string();
    let wire = parse_response(&body, &request());
    assert!(
        wire.as_ref()
            .is_ok_and(|w| w.answers["disclosed"].is_refusal())
    );
    assert!(wire.is_ok_and(|w| w.answers["matters"].noul() == Some(0.2)));
}

#[test]
fn parse_response_missing_question_is_a_schema_error() {
    let body = json!({
        "model": "m", "answers": {"matters": {"type": "noul", "noul": 0.2}},
        "usage": {"input_tokens": 1, "output_tokens": 0}
    })
    .to_string();
    assert!(matches!(
        parse_response(&body, &request()),
        Err(LlmError::Schema(_))
    ));
}

#[test]
fn parse_response_noul_outside_unit_interval_is_a_schema_error() {
    let body = answered(json!({"type": "noul", "noul": 1.7})).to_string();
    assert!(matches!(
        parse_response(&body, &request()),
        Err(LlmError::Schema(_))
    ));
}

#[tokio::test]
async fn decide_success_records_answers_cost_and_served_model() -> anyhow::Result<()> {
    let server = served(200, answered(json!({"type": "noul", "noul": 0.9}))).await;
    let sink = Arc::new(TestSink::default());
    let client = client_for(&server)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::Decision);
    let answer = client.decide(&request(), &context()).await.result?;
    assert_eq!(answer.noul("disclosed"), Some(0.9));
    assert_eq!(answer.refusals(), 0);
    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.role, LlmRole::Decision);
    assert_eq!(call.outcome, LlmCallOutcome::Ok);
    assert_eq!(call.model, "typesafe/jev-1.14-20261101");
    assert_eq!(call.prompt_tokens, Some(344));
    assert!(call.cost_usd.is_some_and(|c| (c - 0.0000144).abs() < 1e-12));
    let stored = call.response_body.clone().unwrap_or_default();
    assert_eq!(stored["answers"]["disclosed"]["noul"], json!(0.9));
    assert_eq!(
        call.request_body.clone().unwrap_or_default()["questions"]["matters"]["type"],
        json!("noul")
    );
    Ok(())
}

#[tokio::test]
async fn decide_malformed_reply_is_recorded_as_schema_error_and_returned() -> anyhow::Result<()> {
    let server = served(200, json!({"answers": "not a map"})).await;
    let sink = Arc::new(TestSink::default());
    let client = client_for(&server)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::Decision);
    let result = client.decide(&request(), &context()).await.result;
    assert!(matches!(result, Err(LlmError::Schema(_))));
    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].outcome, LlmCallOutcome::SchemaError);
    assert_eq!(
        calls[0].response_body.clone().unwrap_or_default()["answers"],
        json!("not a map")
    );
    Ok(())
}

#[tokio::test]
async fn decide_http_error_is_recorded_once_and_not_retried() -> anyhow::Result<()> {
    let server = served(500, json!({"error": {"code": 500}})).await;
    let sink = Arc::new(TestSink::default());
    let client = client_for(&server)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::Decision);
    let result = client.decide(&request(), &context()).await.result;
    assert!(matches!(result, Err(LlmError::Http { status: 500 })));
    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].outcome, LlmCallOutcome::HttpError);
    assert_eq!(calls[0].http_status, Some(500));
    assert_eq!(server.received_requests().await.map_or(0, |r| r.len()), 1);
    Ok(())
}

#[tokio::test]
async fn decide_with_no_key_fails_without_a_call() -> anyhow::Result<()> {
    let server = served(200, answered(json!({"type": "noul", "noul": 0.9}))).await;
    let sink = Arc::new(TestSink::default());
    let client = client_for(&server)?
        .with_api_key(String::new())?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::Decision);
    assert!(client.decide(&request(), &context()).await.result.is_err());
    assert!(sink.recorded().is_empty());
    assert_eq!(server.received_requests().await.map_or(0, |r| r.len()), 0);
    Ok(())
}

#[tokio::test]
async fn decide_refused_by_the_limiter_is_recorded_as_a_refusal() -> anyhow::Result<()> {
    let server = served(200, answered(json!({"type": "noul", "noul": 0.9}))).await;
    let sink = Arc::new(TestSink::with_spend(LlmRole::Decision, 1_000.0));
    let limiter =
        crate::llm::limiter::CallLimiter::with_limits(crate::llm::limiter::LimiterLimits {
            daily_spend_ceiling_usd: 1.0,
            ..crate::llm::limiter::LimiterLimits::compiled_defaults()
        });
    limiter.set_spend_source(Some(sink.clone()));
    let client = client_with_limiter(&server, limiter)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::Decision);
    let result = client.decide(&request(), &context()).await.result;
    assert!(matches!(result, Err(LlmError::SpendCeilingExceeded { .. })));
    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].outcome, LlmCallOutcome::RefusedSpend);
    assert_eq!(server.received_requests().await.map_or(0, |r| r.len()), 0);
    Ok(())
}
