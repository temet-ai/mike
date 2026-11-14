use chrono::{DateTime, Utc};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::web_repo::WebPersist as _;
use uuid::Uuid;

use crate::turn_facts::{Fixture, NewTurn, Req, dev_split, fixture, insert_request, insert_turn};

async fn closed_turn(f: &Fixture, request_id: i64, at_secs: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(request_id, at_secs, "fix the parser")).await?;
    let mut turn = NewTurn::closed(request_id, request_id, at_secs, at_secs + 10);
    turn.final_text = Some("Fixed it and ran the tests.");
    insert_turn(f, &turn).await
}

async fn operator_label(
    f: &Fixture,
    turn: i64,
    outcome: &str,
    reaction: &str,
    delivery: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.operator_turns
            SET outcome_id = (SELECT id FROM memory.turn_outcomes WHERE code = $2),
                outcome_source = 'operator'
          WHERE id = $1",
    )
    .bind(turn)
    .bind(outcome)
    .execute(f.handle.pool())
    .await?;
    dev_split(f, turn).await?;
    for (axis, label) in [("reaction", reaction), ("delivery", delivery)] {
        sqlx::query(
            "INSERT INTO eval.labels
                 (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis,
                  label, tier, method, split, source)
             SELECT 'turn', $1, $1, s.id, s.session_uuid, $2, $3, 'gold', 'operator_click', 'dev', 'test'
               FROM sessions.sessions s WHERE s.id = $4",
        )
        .bind(turn)
        .bind(axis)
        .bind(label)
        .bind(f.session_id)
        .execute(f.handle.pool())
        .await?;
    }
    Ok(())
}

async fn intervention(
    f: &Fixture,
    steerer_kind: &str,
    payload: &str,
    channel: Option<&str>,
) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO steering.interventions
             (steerer_id, session_id, verdict_id, mode_id, authority_id, payload_text,
              delivery_channel, wake_state)
         SELECT st.id, $1, v.id,
                (SELECT id FROM steering.modes ORDER BY id LIMIT 1),
                (SELECT id FROM steering.authorities ORDER BY id LIMIT 1),
                $3, $4, CASE WHEN $4::text IS NULL THEN NULL ELSE 'pushed' END
           FROM steering.steerers st, steering.verdicts v
          WHERE st.kind = $2 AND v.code = 'fired'
          ORDER BY st.id
          LIMIT 1
         RETURNING uuid",
    )
    .bind(f.session_id)
    .bind(steerer_kind)
    .bind(payload)
    .bind(channel)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn evaluation(
    f: &Fixture,
    steerer_kind: &str,
    turn: i64,
    intervention: Uuid,
    question: Option<&str>,
    label: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO steering.evaluations
             (steerer_id, session_id, operator_turn_id, status, intervention_uuid, mode, question,
              trigger_point, resolved_at, outcome_label, label_source, labelled_at, label_evidence)
         SELECT st.id, $1, $2, 'resolved_correction', $3, 'live', $4, 'turn_end', now(), $5::text,
                CASE WHEN $5::text IS NULL THEN NULL ELSE 'operator' END,
                CASE WHEN $5::text IS NULL THEN NULL ELSE now() END,
                CASE WHEN $5::text IS NULL THEN NULL ELSE '{}'::jsonb END
           FROM steering.steerers st
          WHERE st.kind = $6
          ORDER BY st.id
          LIMIT 1",
    )
    .bind(f.session_id)
    .bind(turn)
    .bind(intervention)
    .bind(question)
    .bind(label)
    .bind(steerer_kind)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn ledger(f: &Fixture, request_id: i64, kind: &str, source: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.ask_ledger_entries
             (session_id, kind, content, request_id, kind_source, keyword_kind)
         VALUES ($1, $2, 'that is still broken', $3, $4, 'clarification')",
    )
    .bind(f.session_id)
    .bind(kind)
    .bind(request_id)
    .bind(source)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

type Counts = (i64, i64, i64, i64, i64, i64, i64, i64);

async fn row_counts(f: &Fixture) -> anyhow::Result<Counts> {
    Ok(sqlx::query_as(
        "SELECT (SELECT count(*) FROM decisions.records),
                (SELECT count(*) FROM decisions.outcomes),
                (SELECT count(*) FROM llm.calls),
                (SELECT count(*) FROM steering.evaluations),
                (SELECT count(*) FROM steering.interventions),
                (SELECT count(*) FROM memory.operator_turns),
                (SELECT count(*) FROM memory.ask_ledger_entries),
                (SELECT count(*) FROM eval.labels)",
    )
    .fetch_one(f.handle.pool())
    .await?)
}

async fn seed(f: &Fixture) -> anyhow::Result<(i64, Uuid, Uuid)> {
    let labelled = closed_turn(f, 9700, 0).await?;
    operator_label(f, labelled, "corrected", "corrected", "delivered").await?;
    let unlabelled = closed_turn(f, 9710, 100).await?;

    ledger(f, 9800, "correction", "operator").await?;
    ledger(f, 9801, "correction", "keyword").await?;
    ledger(f, 9802, "initial", "operator").await?;

    let fire = intervention(f, "verifier", "You did not run the tests.", None).await?;
    evaluation(f, "verifier", labelled, fire, None, Some("wrong")).await?;
    let unlabelled_fire = intervention(f, "verifier", "Not labelled.", None).await?;
    evaluation(f, "verifier", unlabelled, unlabelled_fire, None, None).await?;

    let wake = intervention(f, "turn_guard", "You said you would wait.", Some("wake")).await?;
    evaluation(
        f,
        "turn_guard",
        labelled,
        wake,
        Some("unbacked_wait"),
        Some("right"),
    )
    .await?;
    let append = intervention(f, "turn_guard", "Appended, not woken.", None).await?;
    evaluation(
        f,
        "turn_guard",
        labelled,
        append,
        Some("relapse"),
        Some("right"),
    )
    .await?;
    Ok((labelled, fire, wake))
}

#[tokio::test]
async fn replay_readers_return_the_labelled_items_and_write_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (labelled, fire, wake) = seed(&f).await?;
    let before = row_counts(&f).await?;
    let reader = PersistHandle::connect_read_only(&f.db.database_url).await?;
    let since = DateTime::<Utc>::UNIX_EPOCH;

    let turns = reader.web().replay_turn_judge_items(since, 100).await?;
    assert_eq!(turns.len(), 1, "only the operator-labelled turn: {turns:?}");
    assert_eq!(turns[0].turn_id, labelled);
    assert_eq!(turns[0].outcome, "corrected");
    assert_eq!(turns[0].gold_reaction.as_deref(), Some("corrected"));
    assert_eq!(turns[0].gold_delivery.as_deref(), Some("delivered"));
    assert_eq!(turns[0].live_reaction, None);

    let corrections = reader.web().replay_correction_items(since, 100).await?;
    assert_eq!(
        corrections.len(),
        1,
        "only the operator-set correction: {corrections:?}"
    );
    assert_eq!(corrections[0].request_id, 9800);
    assert_eq!(corrections[0].kind, "correction");
    assert_eq!(
        corrections[0].keyword_kind.as_deref(),
        Some("clarification")
    );

    let fires = reader.web().replay_verifier_items(since, 100).await?;
    assert_eq!(fires.len(), 1, "only the labelled verifier fire: {fires:?}");
    assert_eq!(fires[0].intervention_uuid, fire);
    assert_eq!(fires[0].label, "wrong");
    assert_eq!(fires[0].nudge, "You did not run the tests.");
    assert_eq!(fires[0].operator_turn_id, labelled);
    assert_eq!(fires[0].turn_end, "Fixed it and ran the tests.");

    let wakes = reader.web().replay_wake_items(since, 100).await?;
    assert_eq!(wakes.len(), 1, "only the labelled wake fire: {wakes:?}");
    assert_eq!(wakes[0].intervention_uuid, wake);
    assert_eq!(wakes[0].question.as_deref(), Some("unbacked_wait"));
    assert_eq!(wakes[0].label, "right");

    assert_eq!(reader.web().replay_verifier_items(since, 0).await?.len(), 0);
    let future = Utc::now() + chrono::Duration::days(1);
    assert!(
        reader
            .web()
            .replay_wake_items(future, 100)
            .await?
            .is_empty()
    );

    assert_eq!(
        row_counts(&f).await?,
        before,
        "a replay read must not write"
    );
    let write = sqlx::query("DELETE FROM steering.evaluations")
        .execute(reader.pool())
        .await;
    assert!(write.is_err(), "the replay connection must refuse writes");
    assert_eq!(row_counts(&f).await?, before);
    Ok(())
}
