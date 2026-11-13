use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use serde_json::{Value, json};

use tower::ServiceExt;

use crate::turn_facts::{Fixture, NewTurn, Req, attach, fixture, insert_request, insert_turn};

use mike::api::manifest::inspect_router;
use mike::config::ProxyConfig;
use mike::state::AppState;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::proposals::{EvidenceWrite, NewCandidateLesson};

async fn router_on(database_url: &str) -> anyhow::Result<Router> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        "https://api.anthropic.com",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:9989",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
    ])?;
    let state = AppState::new(config).await?;
    Ok(inspect_router(state))
}

async fn post(
    router: &Router,
    path: &str,
    body: Value,
    with_origin: bool,
) -> anyhow::Result<(StatusCode, Value)> {
    let mut builder = Request::builder()
        .header("host", "127.0.0.1:9989")
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if with_origin {
        builder = builder.header("origin", "http://127.0.0.1:9990");
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string()))?)
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let parsed = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    Ok((status, parsed))
}

async fn seed_pending_lesson(f: &Fixture, base_id: i64) -> anyhow::Result<(i64, i64)> {
    insert_request(f, &Req::primary(base_id, 0, "please run the tests")).await?;
    let turn = insert_turn(f, &NewTurn::closed(base_id, base_id, 0, 30)).await?;
    attach(f, turn, &[base_id]).await?;
    let verdict_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.turn_verdicts
            (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
             reaction, delivery, confidence, rationale, unmet_ask, evidence_quality, as_of, input_text)
         VALUES ($1, $2, 'live', 'v2', 'judge-input-v2.1', 'm',
                 'corrected', 'delivered', 0.9, 'skipped a step', 'run the tests', 'verified',
                 now(), 'input')
         RETURNING id",
    )
    .bind(turn)
    .bind(f.session_id)
    .fetch_one(f.handle.pool())
    .await?;
    let empty = json!([]);
    let lesson_id = f
        .handle
        .lessons()
        .create_candidate_lesson(
            NewCandidateLesson {
                project_id: f.project_id,
                kind: "correction",
                text: "always run the tests before claiming they pass",
                source_turn_id: turn,
                session_db_id: f.session_id,
                request_id: None,
                embedding: &vec![0.01_f32; 768],
                embedding_model: "test-embedder",
                shape: None,
                relations: &[],
                placement: None,
            },
            EvidenceWrite {
                verdict_id,
                operator_turn_id: turn,
                lesson_id: None,
                similar: &empty,
                model: "test-model",
                cost_usd: None,
                generation_id: None,
                reason: "generalises",
            },
        )
        .await?;
    Ok((lesson_id, turn))
}

async fn lesson_stage(handle: &PersistHandle, lesson_id: i64) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "SELECT ls.code FROM memory.lessons l JOIN memory.lesson_stages ls ON ls.id = l.stage_id WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn approve_sets_stage_daily() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let router = router_on(&f.db.database_url).await?;
    let (lesson_id, _turn) = seed_pending_lesson(&f, 50_100).await?;

    let (status, body) = post(
        &router,
        &format!("/api/improvement/lessons/{lesson_id}/approve"),
        json!({}),
        false,
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["stage"], "daily");
    assert_eq!(lesson_stage(&f.handle, lesson_id).await?, "daily");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reject_with_wrong_archives_without_a_gold_label() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let router = router_on(&f.db.database_url).await?;
    let (lesson_id, _turn) = seed_pending_lesson(&f, 50_200).await?;

    let (status, body) = post(
        &router,
        &format!("/api/improvement/lessons/{lesson_id}/reject"),
        json!({"reason": "wrong"}),
        false,
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["stage"], "archived");
    assert_eq!(lesson_stage(&f.handle, lesson_id).await?, "archived");

    let label_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM eval.labels WHERE tier = 'gold'")
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(label_count, 0);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reject_as_not_a_correction_writes_a_gold_accepted_label() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let router = router_on(&f.db.database_url).await?;
    let (lesson_id, turn) = seed_pending_lesson(&f, 50_300).await?;

    let (status, body) = post(
        &router,
        &format!("/api/improvement/lessons/{lesson_id}/reject"),
        json!({"reason": "not_a_correction"}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["stage"], "archived");

    let (axis, label, tier): (String, String, String) = sqlx::query_as(
        "SELECT axis, label, tier FROM eval.labels WHERE subject_kind = 'turn' AND subject_id = $1",
    )
    .bind(turn)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        (axis.as_str(), label.as_str(), tier.as_str()),
        ("reaction", "accepted", "gold")
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reject_as_not_a_correction_without_an_allowed_origin_is_forbidden() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let router = router_on(&f.db.database_url).await?;
    let (lesson_id, _turn) = seed_pending_lesson(&f, 50_400).await?;

    let (status, _body) = post(
        &router,
        &format!("/api/improvement/lessons/{lesson_id}/reject"),
        json!({"reason": "not_a_correction"}),
        false,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(lesson_stage(&f.handle, lesson_id).await?, "raw");
    f.db.cleanup().await?;
    Ok(())
}
