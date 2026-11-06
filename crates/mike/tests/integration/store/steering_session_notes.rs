use chrono::{Duration, Utc};
use mike::store::persistence::steering::RegradeOlderVersions;
use mike::store::persistence::steering_evaluations::{
    EvaluationLabel, EvaluationMode, EvaluationVerdict, NewEvaluation,
};

use crate::seed::database_now;
use crate::store::operator_turns::support::{OpenTurnFixture, seed_open_turn_fixture};

struct Note {
    evaluation: uuid::Uuid,
    intervention: uuid::Uuid,
    session_db_id: i64,
}

async fn fired_note(f: &OpenTurnFixture) -> anyhow::Result<Note> {
    let steerer_id: i64 =
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE kind = 'verifier' LIMIT 1")
            .fetch_one(&f.pool)
            .await?;
    let session_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&f.session)
            .fetch_one(&f.pool)
            .await?;
    let evaluation = f
        .handle
        .insert_evaluation_dispatched(&NewEvaluation {
            steerer_id,
            session_id: session_db_id,
            operator_turn_id: f.turn_id,
            mode: EvaluationMode::Live,
            bundle_id: None,
            trigger_point: Some("turn_end"),
            trigger_request_id: Some(f.request_id),
        })
        .await?;
    let intervention = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO steering.interventions (steerer_id, session_id, verdict_id, mode_id, authority_id, uuid,
                                             expires_at, payload_text, delivered_request_id)
         SELECT $1, $2, v.id, m.id, a.id, $3, now() + interval '1 hour', 'check the export', $4
           FROM steering.verdicts v CROSS JOIN steering.modes m CROSS JOIN steering.authorities a
          WHERE v.code = 'fired' AND m.code = 'drive' AND a.code = 'command'",
    )
    .bind(steerer_id)
    .bind(session_db_id)
    .bind(intervention)
    .bind(f.request_id)
    .execute(&f.pool)
    .await?;
    sqlx::query(
        "UPDATE requests.requests SET completed_at = now(), operator_turn_id = $2 WHERE id = $1",
    )
    .bind(f.request_id)
    .bind(f.turn_id)
    .execute(&f.pool)
    .await?;
    let verdict = EvaluationVerdict {
        verdict_json: serde_json::json!({"fires": true, "need": "high"}),
        confidence: Some(0.9),
        reason: Some("asked for an export".to_owned()),
    };
    f.handle
        .resolve_evaluation(evaluation, true, Some(intervention), Some(verdict))
        .await?;
    Ok(Note {
        evaluation,
        intervention,
        session_db_id,
    })
}

fn reply_label(note: &Note, label: &str, version: &str) -> EvaluationLabel {
    EvaluationLabel {
        uuid: note.evaluation,
        label: label.to_owned(),
        source: "session_reply".to_owned(),
        evidence: serde_json::json!({"labeller_version": version, "reaction": "declined"}),
    }
}

fn regrade(version: &str, settles_after_secs: f64) -> RegradeOlderVersions<'_> {
    RegradeOlderVersions {
        current_version: version,
        since: Utc::now() - Duration::hours(72),
        reply_settles_after_secs: settles_after_secs,
    }
}

async fn candidates(
    f: &OpenTurnFixture,
    version: &str,
    settles_after_secs: f64,
) -> anyhow::Result<Vec<uuid::Uuid>> {
    Ok(f.handle
        .reply_label_candidates(
            20,
            Utc::now() - Duration::days(14),
            regrade(version, settles_after_secs),
        )
        .await?
        .into_iter()
        .map(|c| c.uuid)
        .collect())
}

async fn done(f: OpenTurnFixture) -> anyhow::Result<()> {
    f.pool.close().await;
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_note_is_graded_only_after_the_reply_turn_has_ended() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let note = fired_note(&f).await?;
    assert!(
        candidates(&f, "v2", 600.0).await?.is_empty(),
        "the reply turn is still open and the reply has only just started"
    );
    sqlx::query("UPDATE memory.operator_turns SET closed_at = now() WHERE id = $1")
        .bind(f.turn_id)
        .execute(&f.pool)
        .await?;
    let ready = f
        .handle
        .reply_label_candidates(20, Utc::now() - Duration::days(14), regrade("v2", 600.0))
        .await?;
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].uuid, note.evaluation);
    assert_eq!(ready[0].reply_turn_id, Some(f.turn_id));
    assert_eq!(ready[0].delivered_request_id, Some(f.request_id));
    done(f).await
}

#[tokio::test]
async fn an_open_reply_turn_is_graded_once_the_reply_has_had_time_to_settle() -> anyhow::Result<()>
{
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let note = fired_note(&f).await?;
    sqlx::query(
        "UPDATE requests.requests SET completed_at = now() - interval '11 minutes' WHERE id = $1",
    )
    .bind(f.request_id)
    .execute(&f.pool)
    .await?;
    assert_eq!(candidates(&f, "v2", 600.0).await?, vec![note.evaluation]);
    done(f).await
}

#[tokio::test]
async fn a_label_from_an_older_labeller_is_graded_again_and_replaced() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let note = fired_note(&f).await?;
    sqlx::query("UPDATE memory.operator_turns SET closed_at = now() WHERE id = $1")
        .bind(f.turn_id)
        .execute(&f.pool)
        .await?;
    assert_eq!(
        f.handle
            .apply_evaluation_labels(&[reply_label(&note, "right", "v1")])
            .await?,
        1
    );
    assert!(candidates(&f, "v1", 600.0).await?.is_empty());
    assert_eq!(candidates(&f, "v2", 600.0).await?, vec![note.evaluation]);

    assert_eq!(
        f.handle
            .apply_evaluation_labels(&[reply_label(&note, "wrong", "v2")])
            .await?,
        1
    );
    assert!(candidates(&f, "v2", 600.0).await?.is_empty());
    assert_eq!(
        f.handle
            .apply_evaluation_labels(&[reply_label(&note, "right", "v2")])
            .await?,
        0,
        "the same labeller version never overwrites its own label"
    );
    let label: Option<String> =
        sqlx::query_scalar("SELECT outcome_label FROM steering.evaluations WHERE uuid = $1")
            .bind(note.evaluation)
            .fetch_one(&f.pool)
            .await?;
    assert_eq!(label.as_deref(), Some("wrong"));
    done(f).await
}

async fn notes_at(
    f: &OpenTurnFixture,
    session_db_id: i64,
    at: chrono::DateTime<Utc>,
) -> anyhow::Result<Vec<mike::store::persistence::steering::SessionNote>> {
    Ok(f.handle
        .session_notes_since(session_db_id, at - Duration::hours(1), at, 10)
        .await?)
}

#[tokio::test]
async fn a_sessions_recent_notes_carry_how_the_session_took_them() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let before_the_note = database_now(&f.pool).await?;
    let note = fired_note(&f).await?;
    let before = notes_at(&f, note.session_db_id, database_now(&f.pool).await?).await?;
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].steerer_name, "verifier");
    assert_eq!(before[0].payload_text.as_deref(), Some("check the export"));
    assert_eq!(before[0].outcome_label, None);

    let before_the_label = database_now(&f.pool).await?;
    f.handle
        .apply_evaluation_labels(&[reply_label(&note, "wrong", "v2")])
        .await?;
    let after = notes_at(&f, note.session_db_id, database_now(&f.pool).await?).await?;
    assert_eq!(after[0].outcome_label.as_deref(), Some("wrong"));
    assert_eq!(after[0].reaction.as_deref(), Some("declined"));

    let earlier = notes_at(&f, note.session_db_id, before_the_label).await?;
    assert_eq!(
        earlier[0].outcome_label, None,
        "a label written later is not known at the earlier moment"
    );
    assert!(
        notes_at(&f, note.session_db_id, before_the_note)
            .await?
            .is_empty(),
        "a note sent later did not exist at the earlier moment"
    );

    sqlx::query("UPDATE steering.interventions SET reverted_at = now() WHERE uuid = $1")
        .bind(note.intervention)
        .execute(&f.pool)
        .await?;
    assert!(
        notes_at(&f, note.session_db_id, database_now(&f.pool).await?)
            .await?
            .is_empty(),
        "a note that was withdrawn does not count against the session"
    );
    assert!(
        notes_at(&f, note.session_db_id + 1_000, database_now(&f.pool).await?)
            .await?
            .is_empty()
    );
    done(f).await
}

#[tokio::test]
async fn a_note_recorded_but_never_sent_does_not_count() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let note = fired_note(&f).await?;
    sqlx::query(
        "UPDATE steering.interventions
            SET wake_state = 'capped_correction', delivered_request_id = NULL
          WHERE uuid = $1",
    )
    .bind(note.intervention)
    .execute(&f.pool)
    .await?;
    assert!(
        notes_at(&f, note.session_db_id, Utc::now())
            .await?
            .is_empty(),
        "the turn guard records a capped correction without sending it"
    );
    done(f).await
}
