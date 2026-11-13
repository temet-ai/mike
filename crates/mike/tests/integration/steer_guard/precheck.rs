use mike::steerer::guard_test_seams::{SeamTurn, evaluate_guard_turn};
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

const ENDING: &str = "I changed the card layout in a.rs. Done.";

fn guard_body() -> Value {
    let answer = |applies: bool| json!({"applies": applies, "confidence": 0.95, "evidence": ["req 1"], "quote": ENDING, "reason": "no check after the edit", "class": "unknown"});
    let content = json!({"questions": {"relapse": answer(false), "unbacked_wait": answer(false), "authorised_handback": answer(false), "unobserved_done": answer(true)}}).to_string();
    json!({"id": "c", "object": "chat.completion", "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}]})
}

fn jev_body(send_side: f64) -> Value {
    let noul = |v: f64| json!({"type": "noul", "noul": v});
    json!({"id": "gen-dec-1", "model": "typesafe/jev-1.14-20261101", "provider": "TypeSafe",
           "answers": {"already_disclosed": noul(send_side), "invites_trial": noul(send_side),
                       "claims_checked": noul(1.0 - send_side), "answer_not_change": noul(1.0 - send_side)},
           "usage": {"input_tokens": 300, "output_tokens": 0, "cost": 0.0000126}})
}

async fn openrouter(decision: ResponseTemplate) -> MockServer {
    mike::llm::reset_global_for_tests();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(guard_body()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(decision)
        .mount(&server)
        .await;
    server
}

async fn live_wait(f: &Fixture) -> anyhow::Result<()> {
    let steerer = f
        .handle
        .steerer()
        .guard_steerer()
        .await?
        .map(|g| g.id)
        .unwrap_or_default();
    sqlx::query("UPDATE steering.question_policies SET mode = 'append', holdback_pct = 0 WHERE steerer_id = $1 AND question = 'unobserved_done'")
        .bind(steerer)
        .execute(f.handle.pool())
        .await?;
    Ok(())
}

async fn unobserved_edit(f: &Fixture, request: i64) -> anyhow::Result<()> {
    let at = f.base + chrono::Duration::seconds(5);
    let use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, created_at, session_id)
         VALUES ($1, $2, 0, 'Edit', $3, $4, $5) RETURNING id",
    )
    .bind(format!("toolu_{request}"))
    .bind(request)
    .bind(json!({"file_path": "/repo/src/a.rs", "old_string": "a", "new_string": "b\nc"}))
    .bind(at)
    .bind(f.session_id)
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO tools.results (request_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes, created_at, session_id)
         VALUES ($1, $2, 0, false, '\"ok\"', 'x', 4, $3, $4)",
    )
    .bind(request)
    .bind(use_id)
    .bind(at)
    .bind(f.session_id)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn wait_turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "fix the card layout")).await?;
    let t = insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await?;
    crate::turn_facts::attach(f, t, &[start]).await?;
    unobserved_edit(f, start).await?;
    let mut row = crate::turn_facts::sample_row(t, f.session_id, chrono::Utc::now());
    row.session_class = "operator".to_owned();
    f.handle.dossier().upsert_turn_facts(&[row]).await?;
    Ok(t)
}

fn seam(f: &Fixture, turn: i64, decisions_mode: Option<&'static str>) -> SeamTurn {
    SeamTurn {
        session_db_id: f.session_id,
        session_uuid: None,
        operator_turn_id: turn,
        quiescence: false,
        agent_id: None,
        text: ENDING.to_owned(),
        toggle_on: true,
        turn_judge_live: false,
        decisions_mode,
    }
}

async fn arm(f: &Fixture, turn: i64) -> anyhow::Result<(Option<String>, Value)> {
    Ok(sqlx::query_as(
        "SELECT arm, verdict_json FROM steering.evaluations
          WHERE operator_turn_id = $1 AND question = 'unobserved_done'",
    )
    .bind(turn)
    .fetch_one(f.handle.pool())
    .await?)
}

type RecordRow = (
    String,
    String,
    Option<String>,
    Option<bool>,
    Option<String>,
    Option<uuid::Uuid>,
);

async fn records(f: &Fixture, turn: i64) -> anyhow::Result<Vec<RecordRow>> {
    Ok(sqlx::query_as(
        "SELECT outcome, mode, applied, fail_open, response_raw, evaluation_uuid
           FROM decisions.records WHERE operator_turn_id = $1 AND point = 'turn_guard_precheck'",
    )
    .bind(turn)
    .fetch_all(f.handle.pool())
    .await?)
}

async fn decision_calls(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == "/alpha/decisions")
        .count()
}

#[tokio::test]
async fn decisions_off_sends_the_fire_and_switching_on_takes_effect_next_turn() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    live_wait(&f).await?;
    let server = openrouter(ResponseTemplate::new(200).set_body_json(jev_body(0.1))).await;
    let off_turn = wait_turn(&f, 81_001).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, off_turn, None)).await?;
    let (off_arm, off_verdict) = arm(&f, off_turn).await?;
    assert_eq!(off_arm.as_deref(), Some("treated"), "{off_verdict}");
    assert!(off_verdict.get("precheck").is_none());
    assert_eq!(decision_calls(&server).await, 0);
    assert!(records(&f, off_turn).await?.is_empty());

    let on_turn = wait_turn(&f, 81_101).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, on_turn, Some("on"))).await?;
    assert_eq!(
        decision_calls(&server).await,
        1,
        "the toggle takes effect with no restart"
    );
    Ok(())
}

#[tokio::test]
async fn decisions_on_holds_a_fire_the_operator_would_accept_and_records_it() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    live_wait(&f).await?;
    let server = openrouter(ResponseTemplate::new(200).set_body_json(jev_body(0.1))).await;
    let turn = wait_turn(&f, 82_001).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn, Some("on"))).await?;
    let (held, verdict) = arm(&f, turn).await?;
    assert_eq!(held.as_deref(), Some("precheck_held"), "{verdict}");
    assert_eq!(verdict["precheck"]["verdict"]["send"], json!(false));
    let rows = records(&f, turn).await?;
    assert_eq!(rows.len(), 1);
    let (outcome, mode, applied, fail_open, raw, evaluation) = &rows[0];
    assert_eq!((outcome.as_str(), mode.as_str()), ("ok", "on"));
    assert_eq!(applied.as_deref(), Some("held"));
    assert_eq!(*fail_open, Some(false));
    assert!(
        raw.as_deref()
            .is_some_and(|r| r.contains("already_disclosed"))
    );
    assert!(evaluation.is_some());

    sqlx::query("UPDATE memory.turn_facts SET next_state = 'resolved', next_genuine_request_id = max_request_id + 1 WHERE operator_turn_id = $1")
        .bind(turn)
        .execute(f.handle.pool())
        .await?;
    let steerer = f
        .handle
        .steerer()
        .guard_steerer()
        .await?
        .map(|g| g.id)
        .unwrap_or_default();
    let candidates = f
        .handle
        .steerer()
        .guard_label_candidates(steerer, 200, false)
        .await?;
    assert!(
        candidates.iter().any(|c| c.operator_turn_id == turn),
        "a held fire is still labelled"
    );
    Ok(())
}

#[tokio::test]
async fn decisions_on_with_a_failing_jev_sends_like_off_and_records_the_failure()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    live_wait(&f).await?;
    for (start, reply, expected) in [
        (
            83_001,
            ResponseTemplate::new(500).set_body_json(json!({"error": {"code": 500}})),
            "http_error",
        ),
        (
            83_101,
            ResponseTemplate::new(200).set_body_string("{\"answers\": 7}"),
            "schema_error",
        ),
    ] {
        let server = openrouter(reply).await;
        let turn = wait_turn(&f, start).await?;
        evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn, Some("on"))).await?;
        let (sent, verdict) = arm(&f, turn).await?;
        assert_eq!(sent.as_deref(), Some("treated"), "{expected}: {verdict}");
        let rows = records(&f, turn).await?;
        assert_eq!(rows.len(), 1, "{expected}");
        assert_eq!(rows[0].0, expected);
        assert_eq!(rows[0].2.as_deref(), Some("sent"));
        assert_eq!(rows[0].3, Some(true));
        assert!(rows[0].4.is_some(), "{expected}: the raw reply is kept");
    }
    Ok(())
}
