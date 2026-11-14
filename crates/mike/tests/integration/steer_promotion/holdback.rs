use mike::steerer::guard_test_seams::{SeamTurn, arm_draw, evaluate_guard_turn};
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

const ENDING: &str = "The build is running. I'll report back when it lands.";

async fn openrouter() -> MockServer {
    mike::llm::reset_global_for_tests();
    let answer = |applies: bool| json!({"applies": applies, "confidence": 0.95, "evidence": ["req 1"], "quote": ENDING, "reason": "r", "class": "unknown"});
    let content = json!({"questions": {"relapse": answer(false), "unbacked_wait": answer(true), "authorised_handback": answer(false), "unobserved_done": answer(false)}}).to_string();
    let body = json!({"id": "c", "object": "chat.completion", "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}]});
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn wait_turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "rebuild and tell me")).await?;
    let t = insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await?;
    let mut row = crate::turn_facts::sample_row(t, f.session_id, chrono::Utc::now());
    row.session_class = "operator".to_owned();
    f.handle.dossier().upsert_turn_facts(&[row]).await?;
    Ok(t)
}

#[tokio::test]
async fn held_back_would_fire_is_live_with_no_intervention_and_gets_a_label_candidate()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let server = openrouter().await;
    let steerer = f
        .handle
        .steerer()
        .guard_steerer()
        .await?
        .map(|g| g.id)
        .unwrap_or_default();
    let salt = uuid::Uuid::from_u128(12_345);
    sqlx::query("UPDATE steering.question_policies SET mode = 'wake', holdback_pct = 50, holdback_salt = $2 WHERE steerer_id = $1 AND question = 'unbacked_wait'")
        .bind(steerer)
        .bind(salt)
        .execute(f.handle.pool())
        .await?;
    let mut turn_id = 0;
    for start in 70_001..70_040 {
        let t = wait_turn(&f, start).await?;
        if arm_draw(salt, steerer, "unbacked_wait", t).is_some_and(|d| d < 0.5) {
            turn_id = t;
            break;
        }
    }
    anyhow::ensure!(turn_id > 0, "no held-back anchor in 39 turns");
    let seam = SeamTurn {
        session_db_id: f.session_id,
        session_uuid: None,
        operator_turn_id: turn_id,
        quiescence: false,
        agent_id: None,
        text: ENDING.to_owned(),
        toggle_on: true,
        turn_judge_live: false,
        decisions_mode: None,
    };
    evaluate_guard_turn(&f.handle, &server.uri(), seam).await?;
    let (mode, arm, intervention, due, verdict): (
        String,
        Option<String>,
        Option<uuid::Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
        Value,
    ) = sqlx::query_as(
        "SELECT mode, arm, intervention_uuid, due_at, verdict_json FROM steering.evaluations
          WHERE operator_turn_id = $1 AND question = 'unbacked_wait'",
    )
    .bind(turn_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(mode, "live");
    assert_eq!(arm.as_deref(), Some("holdback"), "{verdict}");
    assert!(intervention.is_none());
    assert!(
        due.is_some(),
        "a wake-mode holdback gets a due time for reach"
    );
    let interventions: i64 = sqlx::query_scalar("SELECT count(*) FROM steering.interventions")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(interventions, 0);
    sqlx::query("UPDATE memory.turn_facts SET next_state = 'resolved', next_genuine_request_id = max_request_id + 1 WHERE operator_turn_id = $1")
        .bind(turn_id)
        .execute(f.handle.pool())
        .await?;
    let candidates = f
        .handle
        .steerer()
        .guard_label_candidates(steerer, 200, false)
        .await?;
    assert!(
        candidates.iter().any(|c| c.operator_turn_id == turn_id),
        "held-back rows are labelled"
    );
    Ok(())
}

async fn notes_already_sent(f: &Fixture, steerer: i64, count: i32) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO steering.interventions
                (steerer_id, session_id, verdict_id, mode_id, authority_id, uuid, expires_at,
                 payload_text, pushed_at, created_at)
         SELECT $1, $2, v.id, m.id, a.id, gen_random_uuid(), now() + interval '10 minutes',
                'an earlier note', now() - interval '1 minute', now() - interval '1 minute'
           FROM steering.verdicts v CROSS JOIN steering.modes m CROSS JOIN steering.authorities a
          CROSS JOIN generate_series(1, $3)
          WHERE v.code = 'fired' AND m.code = 'augment' AND a.code = 'redirect'",
    )
    .bind(steerer)
    .bind(f.session_id)
    .bind(count)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn live_decision(f: &Fixture, turn_id: i64) -> anyhow::Result<(Option<String>, Value)> {
    let (arm, verdict): (Option<String>, Value) = sqlx::query_as(
        "SELECT arm, verdict_json FROM steering.evaluations
          WHERE operator_turn_id = $1 AND question = 'unbacked_wait'",
    )
    .bind(turn_id)
    .fetch_one(f.handle.pool())
    .await?;
    Ok((arm, verdict["live"]["unbacked_wait"].clone()))
}

#[tokio::test]
async fn a_session_at_its_note_limit_is_still_judged_but_gets_no_guard_note() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let server = openrouter().await;
    let steerer = f
        .handle
        .steerer()
        .guard_steerer()
        .await?
        .map(|g| g.id)
        .unwrap_or_default();
    sqlx::query("UPDATE steering.question_policies SET mode = 'wake', holdback_pct = 0 WHERE steerer_id = $1 AND question = 'unbacked_wait'")
        .bind(steerer)
        .execute(f.handle.pool())
        .await?;
    let seam = |turn_id: i64| SeamTurn {
        session_db_id: f.session_id,
        session_uuid: None,
        operator_turn_id: turn_id,
        quiescence: false,
        agent_id: None,
        text: ENDING.to_owned(),
        toggle_on: true,
        turn_judge_live: false,
        decisions_mode: None,
    };

    let quiet_session_turn = wait_turn(&f, 71_001).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(quiet_session_turn)).await?;
    let (arm, decision) = live_decision(&f, quiet_session_turn).await?;
    assert_eq!(arm.as_deref(), Some("treated"), "{decision}");
    assert_eq!(decision["decision"], "deliver");

    notes_already_sent(&f, steerer, 3).await?;
    let nagged_session_turn = wait_turn(&f, 71_002).await?;
    evaluate_guard_turn(&f.handle, &server.uri(), seam(nagged_session_turn)).await?;
    let (arm, decision) = live_decision(&f, nagged_session_turn).await?;
    assert_eq!(decision["decision"], "exempt", "{decision}");
    assert_eq!(decision["reason"], "session_note_limit");
    assert_eq!(arm, None, "a refused note joins neither arm of the trial");
    Ok(())
}
