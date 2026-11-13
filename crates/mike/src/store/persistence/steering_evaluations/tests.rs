use super::{EvaluationMode, EvaluationVerdict, NewEvaluation};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::test_support::{self, TestDbHandle};

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
    pool: sqlx::PgPool,
    session: String,
    session_db_id: i64,
    turn_id: i64,
    verifier_id: i64,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let project_id = test_support::seed_project(&pool, "w117-evaluations").await?;
    let session = uuid::Uuid::new_v4().to_string();
    let spare = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session, &spare, project_id).await?;
    test_support::seed_session_turns(&pool, &session, project_id, 1).await?;
    let (session_db_id, turn_id): (i64, i64) = sqlx::query_as(
        "SELECT s.id, t.id
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;
    let verifier_id: i64 =
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE kind = 'verifier' LIMIT 1")
            .fetch_one(&pool)
            .await?;
    Ok(Some(Fixture {
        db,
        handle,
        pool,
        session,
        session_db_id,
        turn_id,
        verifier_id,
    }))
}

fn new_evaluation(f: &Fixture) -> NewEvaluation<'static> {
    NewEvaluation {
        steerer_id: f.verifier_id,
        session_id: f.session_db_id,
        operator_turn_id: f.turn_id,
        mode: EvaluationMode::Live,
        bundle_id: None,
        trigger_point: Some("turn_end"),
        trigger_request_id: None,
    }
}

async fn verdict_columns(
    f: &Fixture,
    uuid: uuid::Uuid,
) -> anyhow::Result<(Option<serde_json::Value>, Option<f64>, Option<String>)> {
    Ok(sqlx::query_as(
        "SELECT verdict_json, confidence, reason FROM steering.evaluations WHERE uuid = $1",
    )
    .bind(uuid)
    .fetch_one(&f.pool)
    .await?)
}

#[tokio::test]
async fn dispatched_then_resolved_pass_round_trip() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let eval_uuid = f
        .handle
        .steerer()
        .insert_evaluation_dispatched(&new_evaluation(&f))
        .await?;
    let Some(mid) = f
        .handle
        .steerer()
        .get_latest_evaluation_for_session(f.session.parse()?, "verifier")
        .await?
    else {
        anyhow::bail!("a dispatched row must be visible before resolution");
    };
    assert_eq!(mid.uuid, eval_uuid);
    assert_eq!(mid.status, "dispatched");
    assert!(
        f.handle
            .steerer()
            .resolve_evaluation(eval_uuid, false, None, None)
            .await?
    );
    let Some(resolved) = f
        .handle
        .steerer()
        .get_latest_evaluation_for_session(f.session.parse()?, "verifier")
        .await?
    else {
        anyhow::bail!("the resolved row must still be the latest evaluation");
    };
    assert_eq!(resolved.status, "resolved_pass");
    assert!(resolved.resolved_at.is_some());
    f.pool.close().await;
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resolved_pass_with_verdict_writes_all_three_columns() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let eval_uuid = f
        .handle
        .steerer()
        .insert_evaluation_dispatched(&new_evaluation(&f))
        .await?;
    let verdict = EvaluationVerdict {
        verdict_json: serde_json::json!({"label": "pass", "steps_checked": 3}),
        confidence: Some(0.87),
        reason: Some("the turn showed a passing test run".to_string()),
    };
    f.handle
        .steerer()
        .resolve_evaluation(eval_uuid, false, None, Some(verdict))
        .await?;
    let (json, confidence, reason) = verdict_columns(&f, eval_uuid).await?;
    assert_eq!(
        json,
        Some(serde_json::json!({"label": "pass", "steps_checked": 3}))
    );
    assert_eq!(confidence, Some(0.87));
    assert_eq!(
        reason.as_deref(),
        Some("the turn showed a passing test run")
    );
    f.pool.close().await;
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resolved_pass_with_no_verdict_records_that_nothing_was_judged() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let eval_uuid = f
        .handle
        .steerer()
        .insert_evaluation_dispatched(&new_evaluation(&f))
        .await?;
    f.handle
        .steerer()
        .resolve_evaluation(eval_uuid, false, None, None)
        .await?;
    let (json, confidence, reason) = verdict_columns(&f, eval_uuid).await?;
    assert_eq!(
        json,
        Some(serde_json::json!({"judged": false, "abstention": "no_judgement_recorded"}))
    );
    assert_eq!(confidence, None);
    assert_eq!(
        reason.as_deref(),
        Some("not judged: the caller recorded no judgement")
    );
    f.pool.close().await;
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn shadow_rows_are_not_the_validation_status() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let shadow = NewEvaluation {
        mode: EvaluationMode::Shadow,
        ..new_evaluation(&f)
    };
    f.handle
        .steerer()
        .insert_evaluation_dispatched(&shadow)
        .await?;
    let found = f
        .handle
        .steerer()
        .get_latest_evaluation_for_session(f.session.parse()?, "verifier")
        .await?;
    assert!(found.is_none());
    f.pool.close().await;
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn no_evaluation_history_returns_none() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let found = f
        .handle
        .steerer()
        .get_latest_evaluation_for_session(f.session.parse()?, "verifier")
        .await?;
    assert!(found.is_none());
    f.pool.close().await;
    f.db.cleanup().await?;
    Ok(())
}
