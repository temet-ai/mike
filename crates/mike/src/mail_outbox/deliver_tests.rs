use super::Stage;
use super::deliver::{Outcome, deliver};
use super::scripted::{Script, ScriptedSeat, as_recipient, database, queued};
use super::store::{self, StageWrite};

fn stage_names(stages: &[(Stage, Option<uuid::Uuid>, String)]) -> Vec<Stage> {
    stages.iter().map(|(s, _, _)| *s).collect()
}

#[tokio::test]
async fn mail_outbox_idle_codex_recipient_gets_a_new_turn_and_shows_each_stage()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        ..Script::default()
    });
    let recipient = seat.recipient();
    let message = queued(&persist, 1).await?;

    let first = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(first, Outcome::Accepted { .. }), "{first:?}");
    let second = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(second, Outcome::Delivered(_)), "{second:?}");

    let attempts = store::attempts(pool, message).await?;
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    assert_eq!(attempts[0].mode, "start");
    assert_eq!(attempts[0].outcome.as_deref(), Some("accepted"));
    let stages = store::stages(pool, message).await?;
    assert_eq!(
        stage_names(&stages),
        vec![
            Stage::Queued,
            Stage::Accepted,
            Stage::Consumed,
            Stage::ActionStarted,
            Stage::Completed
        ],
        "{stages:#?}"
    );
    assert_eq!(seat.copies_of(message), 1);
    let mail = store::load(pool, message)
        .await?
        .ok_or_else(|| anyhow::anyhow!("gone"))?;
    assert_eq!(mail.status, "delivered");
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_busy_codex_recipient_is_steered_into_the_expected_turn() -> anyhow::Result<()>
{
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        ..Script::default()
    });
    let turn = seat.busy();
    let recipient = seat.recipient();
    let message = queued(&persist, 1).await?;

    let first = deliver(pool, as_recipient(&recipient), message).await?;
    assert_eq!(
        first,
        Outcome::Accepted {
            turn_id: turn.clone()
        }
    );
    let second = deliver(pool, as_recipient(&recipient), message).await?;
    let Outcome::Delivered(sighting) = second else {
        anyhow::bail!("not delivered: {second:?}");
    };
    assert_eq!(sighting.turn_id.as_deref(), Some(turn.as_str()));
    assert!(!sighting.has(Stage::Completed), "the turn is still running");
    let attempts = store::attempts(pool, message).await?;
    assert_eq!(attempts[0].mode, "steer");
    assert_eq!(attempts[0].expected_turn_id.as_deref(), Some(turn.as_str()));
    assert_eq!(seat.copies_of(message), 1);
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_turn_ending_during_steer_retries_as_a_new_turn_under_one_message_id()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        end_turn_before_steer: true,
        ..Script::default()
    });
    seat.busy();
    let recipient = seat.recipient();
    let message = queued(&persist, 1).await?;

    let outcome = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(outcome, Outcome::Accepted { .. }), "{outcome:?}");
    let attempts = store::attempts(pool, message).await?;
    let modes: Vec<_> = attempts
        .iter()
        .map(|a| (a.mode.as_str(), a.outcome.as_deref()))
        .collect();
    assert_eq!(
        modes,
        vec![("steer", Some("turn_ended")), ("start", Some("accepted"))]
    );
    assert_ne!(attempts[0].attempt, attempts[1].attempt);
    assert_eq!(seat.copies_of(message), 1);
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_lost_reply_is_reconciled_from_the_thread_and_never_sent_twice()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        lose_reply: true,
        ..Script::default()
    });
    let recipient = seat.recipient();
    let message = queued(&persist, 1).await?;

    let outcome = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(outcome, Outcome::Delivered(_)), "{outcome:?}");
    let attempts = store::attempts(pool, message).await?;
    assert_eq!(attempts.len(), 1, "{attempts:?}");
    assert_eq!(attempts[0].outcome.as_deref(), Some("failed_after_send"));
    assert_eq!(seat.copies_of(message), 1);
    let sends = seat.read(|s| {
        s.calls
            .iter()
            .filter(|c| c.as_str() == "turn/start")
            .count()
    });
    assert_eq!(sends, Some(1));
    let stages = store::stages(pool, message).await?;
    assert!(
        stages.iter().all(|(_, attempt, _)| attempt.is_none()),
        "{stages:#?}"
    );
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_lost_reply_with_unreadable_thread_stays_unresolved_until_the_thread_reads()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        ..Script::default()
    });
    let recipient = seat.recipient();
    let message = queued(&persist, 1).await?;
    let attempt = store::begin_attempt(pool, message, 1, &super::Mode::Start).await?;
    store::finish_attempt(pool, attempt, "failed_after_send", None, Some("timeout")).await?;
    seat.edit(|s| s.read_fails = true);
    let blind = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(blind, Outcome::Unresolved(_)), "{blind:?}");
    assert_eq!(
        store::load(pool, message).await?.map(|m| m.status),
        Some("unresolved".into())
    );
    assert_eq!(
        seat.read(|s| s.calls.iter().filter(|c| c.starts_with("turn/")).count()),
        Some(0)
    );

    seat.edit(|s| s.read_fails = false);
    let sighted = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(
        matches!(sighted, Outcome::Accepted { .. }),
        "not seen, so sent once: {sighted:?}"
    );
    assert_eq!(seat.copies_of(message), 1);
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_failure_before_send_leaves_nothing_delivered_and_the_next_sweep_sends()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        fail_before_send: true,
        ..Script::default()
    });
    let recipient = seat.recipient();
    let message = queued(&persist, 1).await?;

    let first = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(first, Outcome::Waiting(_)), "{first:?}");
    assert_eq!(seat.copies_of(message), 0);
    let second = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(second, Outcome::Accepted { .. }), "{second:?}");
    let outcomes: Vec<_> = store::attempts(pool, message)
        .await?
        .into_iter()
        .map(|a| a.outcome)
        .collect();
    assert_eq!(
        outcomes,
        vec![Some("failed_before_send".into()), Some("accepted".into())]
    );
    assert_eq!(seat.copies_of(message), 1);
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_duplicate_enqueue_and_stage_notifications_change_nothing() -> anyhow::Result<()>
{
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        ..Script::default()
    });
    let recipient = seat.recipient();
    let message = queued(&persist, 1).await?;
    let again = store::enqueue(
        pool,
        &super::store::NewMail {
            message,
            to: uuid::Uuid::new_v4(),
            from: None,
            runtime: super::Runtime::Codex,
            scope: None,
            body: "a second copy",
            generation: 1,
        },
    )
    .await?;
    assert!(!again);

    deliver(pool, as_recipient(&recipient), message).await?;
    let attempt = store::attempts(pool, message).await?[0].attempt;
    let repeat =
        store::record_stage(pool, message, Stage::Accepted, Some(attempt), "repeat").await?;
    assert_eq!(repeat, StageWrite::AlreadyKnown);
    deliver(pool, as_recipient(&recipient), message).await?;
    let settled = deliver(pool, as_recipient(&recipient), message).await?;
    assert_eq!(settled, Outcome::AlreadySettled("delivered".into()));
    assert_eq!(seat.copies_of(message), 1);
    assert_eq!(
        store::load(pool, message).await?.map(|m| m.body),
        Some("please run the check".into())
    );
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_stale_ack_from_an_earlier_generation_cannot_complete_the_message()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        ..Script::default()
    });
    let message = queued(&persist, 1).await?;
    let old = store::begin_attempt(pool, message, 1, &super::Mode::Start).await?;
    store::finish_attempt(pool, old, "accepted", Some("turn-old"), None).await?;
    seat.edit(|s| s.generation = 2);
    store::rebind(pool, message, 2).await?;

    let stale = store::record_stage(pool, message, Stage::Completed, Some(old), "late ack").await?;
    assert_eq!(stale, StageWrite::Stale);
    let newer = store::begin_attempt(pool, message, 2, &super::Mode::Start).await?;
    let late = store::record_stage(pool, message, Stage::Consumed, Some(old), "late ack").await?;
    assert_eq!(late, StageWrite::Stale);
    let fresh = store::record_stage(pool, message, Stage::Accepted, Some(newer), "current").await?;
    assert_eq!(fresh, StageWrite::Recorded);
    let names = stage_names(&store::stages(pool, message).await?);
    assert_eq!(names, vec![Stage::Queued, Stage::Accepted]);
    db.cleanup().await
}
