use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use mike::store::persistence::turn_verdicts::{
    TurnVerdictInsert, VerdictJudgement, VerdictMode, VerdictShape,
};
use serde_json::{Value, json};

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

#[tokio::test]
async fn relapse_evaluation_carries_semantic_key_only_in_record_mode() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let server = openrouter().await;
    let turn = wait_turn(&f, 60_101).await?;
    mike::steerer::guard_test_seams::evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn))
        .await?;
    let relapse: Value = sqlx::query_scalar(
        "SELECT verdict_json FROM steering.evaluations e
           JOIN steering.steerers s ON s.id = e.steerer_id AND s.name = 'turn-guard'
          WHERE e.operator_turn_id = $1 AND e.question = 'relapse'",
    )
    .bind(turn)
    .fetch_one(f.handle.pool())
    .await?;
    assert!(
        relapse["semantic"]["relapse"]["status"].is_string(),
        "record mode (the default) always carries verdict_json.semantic.relapse: {relapse}"
    );

    f.handle
        .upsert_setting("semantic.relapse_evidence", &json!("off"))
        .await?;
    let turn2 = wait_turn(&f, 60_201).await?;
    mike::steerer::guard_test_seams::evaluate_guard_turn(&f.handle, &server.uri(), seam(&f, turn2))
        .await?;
    let relapse2: Value = sqlx::query_scalar(
        "SELECT verdict_json FROM steering.evaluations e
           JOIN steering.steerers s ON s.id = e.steerer_id AND s.name = 'turn-guard'
          WHERE e.operator_turn_id = $1 AND e.question = 'relapse'",
    )
    .bind(turn2)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        relapse2["semantic"],
        Value::Null,
        "semantic.relapse_evidence = off adds nothing"
    );
    Ok(())
}

async fn guard_id(f: &Fixture) -> anyhow::Result<i64> {
    Ok(f.handle
        .steerer()
        .guard_steerer()
        .await?
        .ok_or_else(|| anyhow::anyhow!("turn-guard row not seeded"))?
        .id)
}

async fn spun_up_router(f: &Fixture) -> anyhow::Result<String> {
    let mut state =
        mike::state::AppState::new(mike::config::ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(f.handle.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let router = mike::app::inspect_router_on(state, listener.local_addr()?);
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Ok(base)
}

#[tokio::test]
async fn operator_label_endpoint_writes_label_source_operator() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = guard_id(&f).await?;
    let turn = wait_turn(&f, 60_301).await?;
    let uuid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO steering.evaluations
            (steerer_id, session_id, operator_turn_id, status, mode, question, trigger_point,
             dispatched_at, resolved_at)
         VALUES ($1, $2, $3, 'resolved_pass', 'shadow', 'relapse', 'turn_end', now(), now())
         RETURNING uuid",
    )
    .bind(id)
    .bind(f.session_id)
    .bind(turn)
    .fetch_one(f.handle.pool())
    .await?;

    let base = spun_up_router(&f).await?;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/api/steering/evaluations/{uuid}/label"))
        .header("Origin", "http://127.0.0.1:9990")
        .json(&json!({"label": "right", "note": "it was right"}))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "POST .../label: {} {}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );

    let (outcome_label, label_source): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT outcome_label, label_source FROM steering.evaluations WHERE uuid = $1",
    )
    .bind(uuid)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(outcome_label.as_deref(), Some("right"));
    assert_eq!(
        label_source.as_deref(),
        Some("operator"),
        "the operator label endpoint writes label_source = operator"
    );
    Ok(())
}

async fn seed_arm_row(
    f: &Fixture,
    steerer: i64,
    turn: i64,
    status: &str,
    arm: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO steering.evaluations
            (steerer_id, session_id, operator_turn_id, status, mode, arm, holdback_draw,
             arm_anchor_turn_id, question, trigger_point, reach_at_due, dispatched_at, resolved_at)
         VALUES ($1, $2, $3, $4, 'live', $5, 0.5, $3, 'relapse', 'turn_end', 'reachable', now(), now())",
    )
    .bind(steerer)
    .bind(f.session_id)
    .bind(turn)
    .bind(status)
    .bind(arm)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn seed_scored_reaction(f: &Fixture, turn: i64, reaction: &str) -> anyhow::Result<()> {
    f.handle
        .eval()
        .insert_turn_verdict(TurnVerdictInsert {
            turn_id: turn,
            mode: VerdictMode::Live,
            bundle_id: None,
            rubric_version: "v2-2026-09-23",
            input_builder_version: "judge-input-v2.1",
            slices: Vec::new(),
            model: "google/gemini-3.5-flash",
            judgement: VerdictJudgement {
                shape: VerdictShape::TwoAxis {
                    reaction,
                    delivery: "delivered",
                    failure_tags: None,
                    severity: None,
                },
                confidence: 0.9,
                rationale: "the operator moved on",
                unmet_ask: "",
                evidence_quality: "asserted",
            },
            next_genuine_request_id: None,
            as_of: chrono::Utc::now(),
            input_text: "OPERATOR REQUEST",
            usage: Default::default(),
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn guard_effect_route_returns_arm_counts_and_status() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = guard_id(&f).await?;
    let turn = wait_turn(&f, 60_401).await?;
    seed_arm_row(&f, id, turn, "resolved_pass", "holdback").await?;
    seed_arm_row(&f, id, turn, "resolved_correction", "treated").await?;
    seed_scored_reaction(&f, turn, "accepted").await?;

    let base = spun_up_router(&f).await?;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/api/steering/guard/effect"))
        .send()
        .await?;
    anyhow::ensure!(resp.status().is_success(), "status {}", resp.status());
    let body: Value = resp.json().await?;
    assert!(
        body["read_point_holdback"].is_number(),
        "read_point_holdback is the shared threshold: {body}"
    );
    let questions = body["questions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("questions is not an array: {body}"))?;
    assert_eq!(questions.len(), 4, "one EffectEstimate per Question::ALL");
    let relapse = questions
        .iter()
        .find(|q| q["question"] == "relapse")
        .ok_or_else(|| anyhow::anyhow!("no relapse question in {questions:?}"))?;
    assert_eq!(relapse["secondary"]["treated"]["n"], 1);
    assert_eq!(relapse["secondary"]["holdback"]["n"], 1);
    assert!(
        relapse["status"] == "readable" || relapse["status"] == "monitoring",
        "status is the guard's own readable/monitoring vocabulary: {relapse}"
    );
    Ok(())
}

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
    json!({"id": "chatcmpl-guard-evidence", "object": "chat.completion",
           "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}]})
}

async fn openrouter() -> wiremock::MockServer {
    mike::llm::reset_global_for_tests();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(guard_response()))
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

fn seam(f: &Fixture, turn: i64) -> mike::steerer::guard_test_seams::SeamTurn {
    mike::steerer::guard_test_seams::SeamTurn {
        session_db_id: f.session_id,
        session_uuid: None,
        operator_turn_id: turn,
        quiescence: false,
        agent_id: None,
        text: WAIT_ENDING.to_owned(),
        toggle_on: true,
        turn_judge_live: false,
        decisions_mode: None,
    }
}
