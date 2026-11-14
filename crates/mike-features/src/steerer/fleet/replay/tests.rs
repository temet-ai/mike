use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::parity::{ParityStatus, RecordedCall, diff_sections, split_sections};
use super::run::{ItemOutcome, Prepared, ReplayMode, ReplaySteerer, evaluate_prepared};
use super::sink::FileSink;
use super::{ReplayItem, replay_client};
use crate::llm::OpenRouterClient;
use crate::llm::usage::{LlmCallOutcome, LlmCallRecord, LlmRole, LlmUsageSink};
use crate::steerer::registry::{Steerer, SteererParams};

fn verifier() -> ReplaySteerer {
    ReplaySteerer {
        steerer: Steerer {
            id: 1,
            uuid: uuid::Uuid::new_v4(),
            name: "verifier".to_owned(),
            display_name: "Verifier".to_owned(),
            description: "test".to_owned(),
            agent_id: 1,
            authority: "redirect".to_owned(),
            mode: "augment".to_owned(),
            model: String::new(),
            priority: 0,
            enabled: true,
            params: SteererParams::default(),
            config: serde_json::json!({}),
            prompt: None,
            kind: "verifier".to_owned(),
            trigger_point: "response".to_owned(),
            invocation: "pattern".to_owned(),
            pattern: Some("done".to_owned()),
            auto_enrol: false,
            project_id: None,
            project_path: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        system: "evaluate".to_owned(),
        note_limits: crate::steerer::note_policy::NoteLimits::from_settings(
            &crate::settings::SettingsOverlay::default(),
        ),
    }
}

fn low_need_body() -> ResponseTemplate {
    let content = json!({
        "fires": true,
        "confidence": 0.9,
        "reason": "r",
        "nudge": "n",
        "need": "low",
        "checklist": [{"criterion": "c", "status": "not_done", "evidence": "req 1"}],
    })
    .to_string();
    ResponseTemplate::new(200)
        .set_body_json(json!({"choices": [{"message": {"content": content}}]}))
}

fn verdict_body(fires: bool, confidence: f64, evidence: &str) -> ResponseTemplate {
    let content = json!({
        "fires": fires,
        "confidence": confidence,
        "reason": "r",
        "nudge": "n",
        "checklist": [{"criterion": "c", "status": "not_done", "evidence": evidence}],
    })
    .to_string();
    ResponseTemplate::new(200)
        .set_body_json(json!({"choices": [{"message": {"content": content}}]}))
}

async fn server_and_client() -> anyhow::Result<(MockServer, OpenRouterClient)> {
    let server = MockServer::start().await;
    let client = replay_client(
        "sk-test".to_owned(),
        "test-model".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Ok((server, client))
}

fn sink_in(dir: &tempfile::TempDir) -> anyhow::Result<Arc<FileSink>> {
    Ok(Arc::new(FileSink::create(&dir.path().join("calls.jsonl"))?))
}

fn prepared(admitted: bool) -> Prepared {
    Prepared {
        message: "user message".to_owned(),
        admitted,
        machine_ask: None,
        notes: None,
    }
}

fn call_record(cost: Option<f64>) -> LlmCallRecord {
    LlmCallRecord {
        role: LlmRole::Steerer,
        model: "m".to_owned(),
        generation_id: None,
        prompt_tokens: Some(10),
        completion_tokens: Some(2),
        reasoning_tokens: None,
        cached_tokens: None,
        cost_usd: cost,
        latency_ms: Some(5),
        outcome: LlmCallOutcome::Ok,
        http_status: Some(200),
        session_id: None,
        steerer_id: None,
        operator_turn_id: None,
        request_body: Some(json!({"messages": []})),
        response_body: None,
    }
}

#[tokio::test]
async fn dry_run_makes_no_http_call() -> anyhow::Result<()> {
    let (server, _client) = server_and_client().await?;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let (outcome, verdict) =
        evaluate_prepared(&ReplayMode::DryRun, &verifier(), &prepared(true)).await;

    assert_eq!(outcome, ItemOutcome::DryRun);
    assert!(verdict.is_none());
    let received = server.received_requests().await.unwrap_or_default();
    assert!(
        received.is_empty(),
        "dry run sent {} requests",
        received.len()
    );
    Ok(())
}

#[tokio::test]
async fn gate_closed_item_makes_no_call() -> anyhow::Result<()> {
    let (server, client) = server_and_client().await?;
    let dir = tempfile::tempdir()?;
    let sink = sink_in(&dir)?;
    let mode = ReplayMode::Execute {
        client: &client,
        sink: &sink,
        max_usd: 15.0,
    };

    let (outcome, _) = evaluate_prepared(&mode, &verifier(), &prepared(false)).await;

    assert_eq!(outcome, ItemOutcome::GateClosed);
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

async fn run_with_responses(
    responses: Vec<ResponseTemplate>,
) -> anyhow::Result<(ItemOutcome, usize)> {
    let (server, client) = server_and_client().await?;
    for response in responses {
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(response)
            .up_to_n_times(1)
            .mount(&server)
            .await;
    }
    let dir = tempfile::tempdir()?;
    let sink = sink_in(&dir)?;
    let mode = ReplayMode::Execute {
        client: &client,
        sink: &sink,
        max_usd: 15.0,
    };
    let (outcome, _) = evaluate_prepared(&mode, &verifier(), &prepared(true)).await;
    let calls = server.received_requests().await.unwrap_or_default().len();
    Ok((outcome, calls))
}

#[tokio::test]
async fn an_ask_the_operator_did_not_write_is_not_judged_as_it_is_not_live() -> anyhow::Result<()> {
    let (server, client) = server_and_client().await?;
    let dir = tempfile::tempdir()?;
    let sink = sink_in(&dir)?;
    let mode = ReplayMode::Execute {
        client: &client,
        sink: &sink,
        max_usd: 1.0,
    };
    let compaction = Prepared {
        machine_ask: Some("Claude Code's request to summarise the conversation"),
        ..prepared(true)
    };

    let (outcome, verdict) = evaluate_prepared(&mode, &verifier(), &compaction).await;

    assert_eq!(
        outcome,
        ItemOutcome::MachineAsk {
            asked_by: "Claude Code's request to summarise the conversation"
        }
    );
    assert!(verdict.is_none());
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "no judge call is made for an ask the operator did not write"
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_fires_only_after_evidence_threshold_and_quorum() -> anyhow::Result<()> {
    let (fired, calls) = run_with_responses(vec![
        verdict_body(true, 0.9, "req 1"),
        verdict_body(true, 0.9, "req 1"),
        verdict_body(true, 0.9, "req 1"),
    ])
    .await?;
    assert_eq!(
        fired,
        ItemOutcome::Fired {
            votes_fire: 3,
            votes_total: 3
        }
    );
    assert_eq!(calls, 3);

    let (unevidenced, calls) = run_with_responses(vec![verdict_body(true, 0.9, "")]).await?;
    assert_eq!(unevidenced, ItemOutcome::NoFire);
    assert_eq!(calls, 1);

    let (low, calls) = run_with_responses(vec![verdict_body(true, 0.2, "req 1")]).await?;
    assert_eq!(low, ItemOutcome::BelowThreshold);
    assert_eq!(calls, 1);

    let (unneeded, calls) = run_with_responses(vec![low_need_body()]).await?;
    assert_eq!(unneeded, ItemOutcome::NeedBelowLevel);
    assert_eq!(calls, 1, "a note that is not needed costs no quorum calls");

    let (missed, calls) = run_with_responses(vec![
        verdict_body(true, 0.9, "req 1"),
        verdict_body(false, 0.9, "req 1"),
        verdict_body(false, 0.9, "req 1"),
    ])
    .await?;
    assert_eq!(
        missed,
        ItemOutcome::QuorumMissed {
            votes_fire: 1,
            votes_total: 3
        }
    );
    assert_eq!(calls, 3);
    Ok(())
}

#[tokio::test]
async fn failed_verdict_call_is_reported_not_raised() -> anyhow::Result<()> {
    let (outcome, _) = run_with_responses(vec![ResponseTemplate::new(400)]).await?;
    assert!(
        matches!(outcome, ItemOutcome::CallFailed { .. }),
        "{outcome:?}"
    );
    Ok(())
}

#[tokio::test]
async fn max_usd_stops_the_run() -> anyhow::Result<()> {
    let (_server, client) = server_and_client().await?;
    let dir = tempfile::tempdir()?;
    let sink = sink_in(&dir)?;
    let mode = ReplayMode::Execute {
        client: &client,
        sink: &sink,
        max_usd: 0.05,
    };
    assert!(!mode.exhausted());
    sink.record(call_record(Some(0.03)));
    assert!(!mode.exhausted());
    sink.record(call_record(Some(0.03)));
    assert!(mode.exhausted());
    assert!(!ReplayMode::DryRun.exhausted());
    Ok(())
}

#[tokio::test]
async fn missing_or_non_finite_cost_stops_the_run() -> anyhow::Result<()> {
    let (_server, client) = server_and_client().await?;
    for bad in [None, Some(f64::NAN), Some(f64::INFINITY)] {
        let dir = tempfile::tempdir()?;
        let sink = sink_in(&dir)?;
        let mode = ReplayMode::Execute {
            client: &client,
            sink: &sink,
            max_usd: 100.0,
        };
        sink.record(call_record(Some(0.01)));
        assert!(!mode.exhausted());
        sink.record(call_record(bad));
        assert!(mode.exhausted(), "{bad:?}");
        assert!(
            (sink.total_usd() - 0.01).abs() < 1e-9,
            "{bad:?} counted as spend"
        );
    }
    Ok(())
}

#[tokio::test]
async fn refused_call_without_cost_does_not_stop_the_run() -> anyhow::Result<()> {
    let (_server, client) = server_and_client().await?;
    let dir = tempfile::tempdir()?;
    let sink = sink_in(&dir)?;
    let mode = ReplayMode::Execute {
        client: &client,
        sink: &sink,
        max_usd: 100.0,
    };
    let mut refused = call_record(None);
    refused.outcome = LlmCallOutcome::RefusedRate;
    sink.record(refused);
    assert!(!mode.exhausted());
    Ok(())
}

#[test]
fn file_sink_records_every_call_and_total() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("calls.jsonl");
    let sink = FileSink::create(&file)?;
    sink.record(call_record(Some(0.01)));
    sink.record(call_record(None));
    sink.record(call_record(Some(0.02)));

    assert_eq!(sink.calls(), 3);
    assert!((sink.total_usd() - 0.03).abs() < 1e-9);
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(&file)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["cost_usd"], json!(0.01));
    assert_eq!(lines[1]["cost_usd"], serde_json::Value::Null);
    assert_eq!(lines[2]["request_body"], json!({"messages": []}));
    assert_eq!(sink.spent_today().of(LlmRole::Steerer), 0.0);
    Ok(())
}

#[test]
fn replay_item_parses_from_a_jsonl_line() -> anyhow::Result<()> {
    let item: ReplayItem = serde_json::from_str(r#"{"turn_id": 7, "trigger_request_id": 42}"#)?;
    assert_eq!((item.turn_id, item.trigger_request_id), (7, 42));
    assert!(serde_json::from_str::<ReplayItem>(r#"{"turn_id": 7}"#).is_err());
    Ok(())
}

const MESSAGE: &str = "User ask:\nfix it\n\nAssistant response to evaluate:\ndone\n\nwith a second paragraph\n\nSession action list (issue #158):\n1. Bash";

#[test]
fn split_sections_keeps_unheaded_paragraphs_with_their_section() {
    let sections = split_sections(MESSAGE);
    let keys: Vec<&str> = sections.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["user_ask", "response", "session_action_list"]);
    assert!(sections[1].1.ends_with("with a second paragraph"));
}

#[test]
fn diff_sections_names_the_first_differing_section_and_offset() {
    assert!(diff_sections(MESSAGE, MESSAGE).is_empty());
    let changed = MESSAGE.replace("1. Bash", "1. Edit");
    let diffs = diff_sections(MESSAGE, &changed);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].section, "session_action_list");
    assert_eq!(
        diffs[0].first_differing_offset,
        "Session action list (issue #158):\n1. ".len()
    );
    assert!(diffs[0].allowed_cause.is_none());

    let context = "Session context:\nState of play: a\nNext: b";
    let other = "Session context:\nState of play: z\nNext: b";
    let diffs = diff_sections(context, other);
    assert!(diffs[0].allowed_cause.is_some());
}

#[test]
fn parity_status_serialises_with_its_tag() -> anyhow::Result<()> {
    let value = serde_json::to_value(ParityStatus::Unmapped {
        reason: "no operator turn before the trigger",
    })?;
    assert_eq!(
        value,
        json!({"status": "unmapped", "reason": "no operator turn before the trigger"})
    );
    Ok(())
}

#[test]
fn split_sections_attributes_headings_after_a_trailing_newline_section() {
    let message = "Assistant response to evaluate:\ndone\n\nTrajectory digest (this turn):\n1. Bash\n\n\nSession action list (issue #158):\n1. Bash\n\n\nAcceptance criteria authored at intake from the operator\n- one\n\nPopulate `checklist` from the criteria above";
    let keys: Vec<String> = split_sections(message)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        keys,
        [
            "response",
            "trajectory_digest",
            "session_action_list",
            "intake_criteria",
            "checklist_instruction"
        ]
    );
}

#[test]
fn diff_sections_blames_the_action_list_not_the_digest_across_a_triple_newline() {
    let recorded =
        "Trajectory digest (x):\n1. Bash\n\n\nSession action list (issue #158):\n1. Bash\n";
    let rebuilt =
        "Trajectory digest (x):\n1. Bash\n\n\nSession action list (issue #158):\n1. Edit\n";
    let diffs = diff_sections(recorded, rebuilt);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].section, "session_action_list");
}

#[test]
fn recorded_call_called_at_is_optional_and_parsed() -> anyhow::Result<()> {
    let with: RecordedCall = serde_json::from_str(
        r#"{"call_id":1,"turn_id":2,"trigger_request_id":3,"request_body":{},"called_at":"2026-09-22T20:57:04.75Z"}"#,
    )?;
    assert!(with.called_at.is_some());
    let without: RecordedCall = serde_json::from_str(
        r#"{"call_id":1,"turn_id":2,"trigger_request_id":3,"request_body":{}}"#,
    )?;
    assert!(without.called_at.is_none());
    Ok(())
}
