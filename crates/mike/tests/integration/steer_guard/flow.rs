use mike::steerer::guard_test_seams::{SeamTurn, evaluate_guard_turn};
use mike::store::persistence::steerer_repo::SteererPersist as _;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

const WAIT_ENDING: &str = "The build is running. I'll report back when it lands.";

fn answer(applies: bool) -> Value {
    json!({"applies": applies, "confidence": 0.95, "evidence": ["req 1"], "quote": WAIT_ENDING,
           "reason": "nothing backs the wait", "class": "unknown"})
}

fn guard_response() -> Value {
    let content = json!({"questions": {
        "relapse": answer(false),
        "unbacked_wait": answer(true),
        "authorised_handback": answer(false),
        "unobserved_done": answer(false),
    }})
    .to_string();
    json!({"id": "chatcmpl-guard", "object": "chat.completion",
           "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}]})
}

async fn openrouter() -> MockServer {
    mike::llm::reset_global_for_tests();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(guard_response()))
        .mount(&server)
        .await;
    server
}

async fn wait_turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(
        f,
        &Req::primary(start, 0, "rebuild and tell me when it is green"),
    )
    .await?;
    insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await
}

fn seam(f: &Fixture, turn: i64, toggle_on: bool) -> SeamTurn {
    SeamTurn {
        session_db_id: f.session_id,
        session_uuid: None,
        operator_turn_id: turn,
        quiescence: false,
        agent_id: None,
        text: WAIT_ENDING.to_owned(),
        toggle_on,
        turn_judge_live: false,
        decisions_mode: None,
    }
}

async fn rows(f: &Fixture, turn: i64) -> anyhow::Result<Vec<(String, String, String, Value)>> {
    Ok(sqlx::query_as(
        "SELECT e.question, e.trigger_point, e.mode, e.verdict_json
           FROM steering.evaluations e
           JOIN steering.steerers s ON s.id = e.steerer_id AND s.name = 'turn-guard'
          WHERE e.operator_turn_id = $1
          ORDER BY e.question, e.trigger_point",
    )
    .bind(turn)
    .fetch_all(f.handle.pool())
    .await?)
}

async fn deliveries(f: &Fixture) -> anyhow::Result<i64> {
    let interventions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM steering.interventions i
           JOIN steering.steerers s ON s.id = i.steerer_id
          WHERE s.name = 'turn-guard'",
    )
    .fetch_one(f.handle.pool())
    .await?;
    let injections: i64 = sqlx::query_scalar("SELECT count(*) FROM remote_control.injections")
        .fetch_one(f.handle.pool())
        .await?;
    Ok(interventions + injections)
}

#[tokio::test]
async fn guard_off_makes_no_call_and_writes_no_row() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let server = openrouter().await;
    let turn = wait_turn(&f, 50_001).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn, false)).await?;
    assert!(rows(&f, turn).await?.is_empty());
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn, true)).await?;
    assert_eq!(
        rows(&f, turn).await?.len(),
        4,
        "switching the toggle on resumes without restart"
    );
    Ok(())
}

#[tokio::test]
async fn guard_on_shadow_would_fire_records_rows_and_delivers_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let server = openrouter().await;
    let turn = wait_turn(&f, 50_101).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn, true)).await?;
    let rows = rows(&f, turn).await?;
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter()
            .all(|(_, trigger, mode, _)| trigger == "turn_end" && mode == "shadow")
    );
    let wait = rows
        .iter()
        .find(|(q, _, _, _)| q == "unbacked_wait")
        .map(|r| &r.3);
    let Some(wait) = wait else {
        anyhow::bail!("no unbacked_wait row in {rows:?}");
    };
    assert_eq!(wait["gate_version"], "gate-v1");
    assert_eq!(wait["gates"]["unbacked_wait"]["arm"], "g2");
    assert!(
        wait["questions"]["unbacked_wait"]["applies"]
            .as_bool()
            .unwrap_or(false)
    );
    assert!(wait["reach"].is_object());
    assert_eq!(wait["semantic"], Value::Null);
    assert!(
        wait["questions"]["authorised_handback"]
            .get("class")
            .is_some(),
        "the JC18 class path resolves"
    );
    assert_eq!(deliveries(&f).await?, 0, "no intervention or injection row");
    Ok(())
}

#[tokio::test]
async fn guard_exempts_agent_request_on_primary_lane() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let server = openrouter().await;
    let turn = wait_turn(&f, 50_201).await?;
    let mut s = seam(&f, turn, true);
    s.agent_id = Some("agent-7".to_owned());
    evaluate_guard_turn(&f.handle, &server.uri(), s).await?;
    let rows = rows(&f, turn).await?;
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|r| r.3["exemption"] == "agent_request"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn guard_asks_unbacked_wait_at_turn_end_and_again_when_quiet() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let server = openrouter().await;
    let turn = wait_turn(&f, 50_301).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn, true)).await?;
    let mut quiet = seam(&f, turn, true);
    quiet.quiescence = true;
    evaluate_guard_turn(&f.handle, &server.uri(), quiet).await?;
    let waits: Vec<String> = rows(&f, turn)
        .await?
        .into_iter()
        .filter(|r| r.0 == "unbacked_wait")
        .map(|r| r.1)
        .collect();
    assert_eq!(waits, vec!["quiescence".to_owned(), "turn_end".to_owned()]);
    Ok(())
}

#[tokio::test]
async fn question_mode_off_is_not_asked() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let server = openrouter().await;
    let id = f
        .handle
        .steerer()
        .guard_steerer()
        .await?
        .map(|g| g.id)
        .unwrap_or_default();
    for q in ["relapse", "authorised_handback", "unobserved_done"] {
        f.handle
            .steerer()
            .set_question_mode(id, q, "off", "cli")
            .await?;
    }
    let turn = wait_turn(&f, 50_401).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn, true)).await?;
    let rows = rows(&f, turn).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "unbacked_wait");
    Ok(())
}
