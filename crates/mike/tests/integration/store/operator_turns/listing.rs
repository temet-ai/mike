use mike::store::persistence::operator_turns::QuiescenceTarget;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use mike::store::persistence::steering::NewIntervention;
use mike::store::persistence::steering_evaluations::{EvaluationMode, NewEvaluation};

use crate::store::operator_turns::support::seed_open_turn_fixture;

async fn verifier_steerer_id(pool: &sqlx::PgPool) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE kind = 'verifier' LIMIT 1")
            .fetch_one(pool)
            .await?,
    )
}

async fn enrol_session(
    pool: &sqlx::PgPool,
    session_uuid: &str,
    steerer_id: i64,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO steering.enrolments (session_id, steerer_id, mode_id, authority_id, enrolled_by)
             SELECT s.id, $2, m.id, a.id, 'test'
               FROM sessions.sessions s, steering.modes m, steering.authorities a
              WHERE s.session_uuid = $1::text::uuid
                AND m.code = 'augment' AND a.code = 'supply_context'
             RETURNING id",
    )
    .bind(session_uuid)
    .bind(steerer_id)
    .fetch_one(pool)
    .await?)
}

async fn idle_for(pool: &sqlx::PgPool, request_id: i64, interval: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE requests.requests SET completed_at = NOW() - $2::interval WHERE id = $1")
        .bind(request_id)
        .bind(interval)
        .execute(pool)
        .await?;
    Ok(())
}

async fn session_db_id(pool: &sqlx::PgPool, session: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(session)
            .fetch_one(pool)
            .await?,
    )
}

fn target(steerer_id: i64, question: Option<&str>) -> QuiescenceTarget {
    QuiescenceTarget {
        steerer_id,
        idle_after_secs: 60,
        question: question.map(str::to_owned),
    }
}

async fn record_evaluation(
    fixture: &crate::store::operator_turns::support::OpenTurnFixture,
    steerer_id: i64,
    question: Option<&str>,
    trigger_point: &'static str,
) -> anyhow::Result<()> {
    let new = NewEvaluation {
        steerer_id,
        session_id: session_db_id(&fixture.pool, &fixture.session).await?,
        operator_turn_id: fixture.turn_id,
        mode: EvaluationMode::Live,
        bundle_id: None,
        trigger_point: Some(trigger_point),
        trigger_request_id: Some(fixture.request_id),
    };
    fixture
        .handle
        .steerer()
        .insert_evaluations_dispatched(&new, &[question.map(str::to_owned)])
        .await?;
    Ok(())
}

#[tokio::test]
async fn list_quiescent_turns_finds_idle_enrolled_candidate_for_target() -> anyhow::Result<()> {
    let Some(fixture) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id = verifier_steerer_id(&fixture.pool).await?;
    enrol_session(&fixture.pool, &fixture.session, steerer_id).await?;
    idle_for(&fixture.pool, fixture.request_id, "1 hour").await?;

    let candidates = fixture
        .handle
        .list_quiescent_turns(&[target(steerer_id, Some("unbacked_wait"))], 50)
        .await?;
    let Some(hit) = candidates.iter().find(|c| c.turn_id == fixture.turn_id) else {
        anyhow::bail!("an idle, enrolled open turn must be a candidate; got {candidates:?}");
    };
    assert_eq!(hit.end_request_id, fixture.request_id);
    assert_eq!(hit.steerer_id, steerer_id);
    assert_eq!(hit.question.as_deref(), Some("unbacked_wait"));
    assert!(hit.idle_seconds >= 3600);
    fixture.pool.close().await;
    fixture.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_quiescent_turns_excludes_unenrolled_session() -> anyhow::Result<()> {
    let Some(fixture) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id = verifier_steerer_id(&fixture.pool).await?;
    idle_for(&fixture.pool, fixture.request_id, "1 hour").await?;
    let candidates = fixture
        .handle
        .list_quiescent_turns(&[target(steerer_id, None)], 50)
        .await?;
    assert!(candidates.iter().all(|c| c.turn_id != fixture.turn_id));
    fixture.pool.close().await;
    fixture.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_quiescent_turns_empty_targets_returns_nothing() -> anyhow::Result<()> {
    let Some(fixture) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id = verifier_steerer_id(&fixture.pool).await?;
    enrol_session(&fixture.pool, &fixture.session, steerer_id).await?;
    idle_for(&fixture.pool, fixture.request_id, "1 hour").await?;
    assert!(
        fixture
            .handle
            .list_quiescent_turns(&[], 50)
            .await?
            .is_empty()
    );
    fixture.pool.close().await;
    fixture.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_quiescent_turns_ignores_turns_idle_more_than_24h() -> anyhow::Result<()> {
    let Some(fixture) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id = verifier_steerer_id(&fixture.pool).await?;
    enrol_session(&fixture.pool, &fixture.session, steerer_id).await?;
    idle_for(&fixture.pool, fixture.request_id, "25 hours").await?;
    let candidates = fixture
        .handle
        .list_quiescent_turns(&[target(steerer_id, None)], 50)
        .await?;
    assert!(candidates.iter().all(|c| c.turn_id != fixture.turn_id));
    fixture.pool.close().await;
    fixture.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_quiescent_turns_excludes_turn_with_same_question_evaluation() -> anyhow::Result<()> {
    let Some(fixture) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id = verifier_steerer_id(&fixture.pool).await?;
    enrol_session(&fixture.pool, &fixture.session, steerer_id).await?;
    idle_for(&fixture.pool, fixture.request_id, "1 hour").await?;
    record_evaluation(&fixture, steerer_id, Some("unbacked_wait"), "quiescence").await?;
    let candidates = fixture
        .handle
        .list_quiescent_turns(&[target(steerer_id, Some("unbacked_wait"))], 50)
        .await?;
    assert!(candidates.iter().all(|c| c.turn_id != fixture.turn_id));
    fixture.pool.close().await;
    fixture.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_quiescent_turns_includes_turn_with_other_question_evaluation() -> anyhow::Result<()> {
    let Some(fixture) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id = verifier_steerer_id(&fixture.pool).await?;
    enrol_session(&fixture.pool, &fixture.session, steerer_id).await?;
    idle_for(&fixture.pool, fixture.request_id, "1 hour").await?;
    record_evaluation(&fixture, steerer_id, Some("relapse"), "quiescence").await?;
    let candidates = fixture
        .handle
        .list_quiescent_turns(&[target(steerer_id, Some("unbacked_wait"))], 50)
        .await?;
    assert!(candidates.iter().any(|c| c.turn_id == fixture.turn_id));
    fixture.pool.close().await;
    fixture.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_quiescent_turns_includes_turn_with_turn_end_evaluation_of_same_question()
-> anyhow::Result<()> {
    let Some(fixture) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id = verifier_steerer_id(&fixture.pool).await?;
    enrol_session(&fixture.pool, &fixture.session, steerer_id).await?;
    idle_for(&fixture.pool, fixture.request_id, "1 hour").await?;
    record_evaluation(&fixture, steerer_id, Some("unbacked_wait"), "turn_end").await?;
    let candidates = fixture
        .handle
        .list_quiescent_turns(&[target(steerer_id, Some("unbacked_wait"))], 50)
        .await?;
    assert!(candidates.iter().any(|c| c.turn_id == fixture.turn_id));
    fixture.pool.close().await;
    fixture.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_quiescent_turns_excludes_turn_with_intervention_for_target() -> anyhow::Result<()> {
    let Some(fixture) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id = verifier_steerer_id(&fixture.pool).await?;
    let enrolment_id = enrol_session(&fixture.pool, &fixture.session, steerer_id).await?;
    idle_for(&fixture.pool, fixture.request_id, "1 hour").await?;
    fixture
        .handle
        .steerer()
        .insert_intervention(NewIntervention {
            steerer_id,
            enrolment_id: Some(enrolment_id),
            session_id: session_db_id(&fixture.pool, &fixture.session).await?,
            verdict_code: "fired",
            mode_code: "augment",
            authority_code: "supply_context",
            trigger_request_id: Some(fixture.request_id),
            confidence: Some(0.9),
            off_thread_topic: None,
            payload_text: Some("already fired"),
            missing_evidence: None,
            model: Some("test-model"),
            anchor_text: None,
            intervention_uuid: uuid::Uuid::new_v4(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
            verdict_json: None,
            operator_turn_id: Some(fixture.turn_id),
            bundle_id: None,
            delivery_channel: None,
            addressed_to: None,
            deliver_after: None,
            wake_state: None,
        })
        .await?;
    let candidates = fixture
        .handle
        .list_quiescent_turns(&[target(steerer_id, None)], 50)
        .await?;
    assert!(candidates.iter().all(|c| c.turn_id != fixture.turn_id));
    fixture.pool.close().await;
    fixture.db.cleanup().await?;
    Ok(())
}
