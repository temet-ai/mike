use std::sync::Arc;

use tokio::sync::RwLock;

use super::super::*;
use super::command_failed_test::seed_closed_turn;
use super::support::{
    evaluate_turn_outcome, make_fleet_ctx, make_verifier_steerer, mock_openrouter_returning,
    seed_project_and_session,
};
use crate::llm::LlmRole;
use crate::steerer::note_policy::NoteRefusal;
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::retention::RetentionPolicy;

struct Session {
    db: TestDbHandle,
    pool: sqlx::PgPool,
    handle: PersistHandle,
    session_uuid: uuid::Uuid,
    session_db_id: i64,
    request_id: i64,
    turn_id: i64,
    steerer: Steerer,
}

struct Judged {
    outcome: LlmCallOutcome,
    judge_calls: usize,
    notes_queued: Vec<String>,
}

fn would_fire(need: &str) -> String {
    serde_json::json!({
        "fires": true,
        "confidence": 0.95,
        "reason": "the export is missing",
        "nudge": "You asked for the export and I do not see it in this turn.",
        "need": need,
        "checklist": [{"criterion": "the export exists", "status": "not_done", "evidence": "req 1"}],
    })
    .to_string()
}

async fn session(name: &str) -> anyhow::Result<Option<Session>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (project_id, session_uuid, session_db_id) = seed_project_and_session(&pool, name).await?;
    let (request_id, turn_id) = seed_closed_turn(&pool, session_db_id, project_id).await?;
    let mut steerer = make_verifier_steerer();
    steerer.id = sqlx::query_scalar(
        "SELECT id FROM steering.steerers WHERE kind = 'verifier' ORDER BY id LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    Ok(Some(Session {
        db,
        pool,
        handle,
        session_uuid,
        session_db_id,
        request_id,
        turn_id,
        steerer,
    }))
}

async fn notes_already_sent(s: &Session, count: i32) -> anyhow::Result<()> {
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
    .bind(s.steerer.id)
    .bind(s.session_db_id)
    .bind(count)
    .execute(&s.pool)
    .await?;
    Ok(())
}

async fn judge_a_done_claim(
    s: &Session,
    verdict: &str,
    turn_is_known: bool,
) -> anyhow::Result<Judged> {
    let (mock, llm) = mock_openrouter_returning(verdict).await?;
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let ctx = make_fleet_ctx(
        store.clone(),
        Some(s.handle.clone()),
        llm.with_role(LlmRole::Steerer),
    );
    let inputs = EvalInputs {
        eval_text: "Done. The importer is finished and all tests pass.".to_string(),
        operator_ask: "build the importer and the export".to_string(),
        last_user_message: "build the importer and the export".to_string(),
        trigger_point: "response",
        judged_turn_id: turn_is_known.then_some(s.turn_id),
        ..Default::default()
    };
    let outcome = evaluate_turn_outcome(
        &ctx,
        &s.steerer,
        s.session_uuid.to_string(),
        turn_is_known.then_some(s.request_id),
        inputs,
    )
    .await;
    let notes_queued = store
        .write()
        .await
        .claim_reminders(&s.session_uuid.to_string())
        .into_iter()
        .map(|reminder| reminder.payload)
        .collect();
    Ok(Judged {
        outcome,
        judge_calls: mock.received_requests().await.unwrap_or_default().len(),
        notes_queued,
    })
}

async fn done(s: Session) -> anyhow::Result<()> {
    s.pool.close().await;
    s.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_session_at_its_limit_is_not_judged_and_gets_no_note() -> anyhow::Result<()> {
    let Some(s) = session("proj-note-limit").await? else {
        return Ok(());
    };
    notes_already_sent(&s, 3).await?;

    let judged = judge_a_done_claim(&s, &would_fire("high"), true).await?;

    assert_eq!(
        judged.outcome.abstention,
        Some(Abstention::Note(NoteRefusal::SessionNoteLimit))
    );
    assert_eq!(
        judged.judge_calls, 0,
        "a session at its limit costs no judge call"
    );
    assert!(judged.notes_queued.is_empty());
    done(s).await
}

#[tokio::test]
async fn the_limit_holds_when_the_turn_of_the_request_is_not_known() -> anyhow::Result<()> {
    let Some(s) = session("proj-note-limit-no-turn").await? else {
        return Ok(());
    };
    notes_already_sent(&s, 3).await?;

    let judged = judge_a_done_claim(&s, &would_fire("high"), false).await?;

    assert_eq!(
        judged.outcome.abstention,
        Some(Abstention::Note(NoteRefusal::SessionNoteLimit))
    );
    assert_eq!(judged.judge_calls, 0);
    assert!(judged.notes_queued.is_empty());
    done(s).await
}

#[tokio::test]
async fn after_one_note_a_second_is_sent_only_when_it_is_needed_more() -> anyhow::Result<()> {
    let Some(s) = session("proj-note-need").await? else {
        return Ok(());
    };
    notes_already_sent(&s, 1).await?;

    let medium = judge_a_done_claim(&s, &would_fire("medium"), true).await?;
    assert_eq!(
        medium.outcome.abstention,
        Some(Abstention::Note(NoteRefusal::NeedBelowLevel))
    );
    assert_eq!(
        medium.judge_calls, 1,
        "a note that is not needed enough costs no quorum calls"
    );
    assert!(medium.notes_queued.is_empty());

    let high = judge_a_done_claim(&s, &would_fire("high"), true).await?;
    assert_eq!(
        high.notes_queued.len(),
        1,
        "abstention: {:?}",
        high.outcome.abstention
    );
    assert!(high.notes_queued[0].contains("You asked for the export"));
    done(s).await
}

#[tokio::test]
async fn a_session_with_no_recent_note_gets_one_of_medium_need_and_none_of_low()
-> anyhow::Result<()> {
    let Some(s) = session("proj-note-first").await? else {
        return Ok(());
    };

    let low = judge_a_done_claim(&s, &would_fire("low"), true).await?;
    assert_eq!(
        low.outcome.abstention,
        Some(Abstention::Note(NoteRefusal::NeedBelowLevel))
    );
    assert!(low.notes_queued.is_empty());

    let medium = judge_a_done_claim(&s, &would_fire("medium"), true).await?;
    assert_eq!(
        medium.notes_queued.len(),
        1,
        "abstention: {:?}",
        medium.outcome.abstention
    );
    done(s).await
}
