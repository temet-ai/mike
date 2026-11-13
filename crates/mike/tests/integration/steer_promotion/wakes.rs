use chrono::{Duration, Utc};
use mike::store::persistence::steerer_repo::SteererPersist as _;

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

async fn pending_wake(
    f: &Fixture,
    turn: i64,
    due_in_secs: i64,
) -> anyhow::Result<(i64, uuid::Uuid)> {
    let uuid = uuid::Uuid::new_v4();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO steering.interventions (steerer_id, session_id, verdict_id, mode_id, authority_id, uuid, expires_at,
                                             payload_text, operator_turn_id, delivery_channel, deliver_after, wake_state)
         SELECT st.id, $1, v.id, m.id, a.id, $2, now() + interval '1 hour', 'nudge', $3, 'wake', $4, 'pending'
           FROM steering.steerers st
           CROSS JOIN steering.verdicts v CROSS JOIN steering.modes m CROSS JOIN steering.authorities a
          WHERE st.name = 'turn-guard' AND v.code = 'fired' AND m.code = 'drive' AND a.code = 'command'
         RETURNING id",
    )
    .bind(f.session_id)
    .bind(uuid)
    .bind(turn)
    .bind(Utc::now() + Duration::seconds(due_in_secs))
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO steering.evaluations (steerer_id, session_id, operator_turn_id, status, mode, question, trigger_point,
                                           dispatched_at, resolved_at, intervention_uuid, arm, holdback_draw, arm_anchor_turn_id)
         SELECT st.id, $1, $2, 'resolved_correction', 'live', 'relapse', 'turn_end', now(), now(), $3, 'treated', 0.9, $2
           FROM steering.steerers st WHERE st.name = 'turn-guard'",
    )
    .bind(f.session_id)
    .bind(turn)
    .bind(uuid)
    .execute(f.handle.pool())
    .await?;
    Ok((id, uuid))
}

async fn turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "stop waiting")).await?;
    insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await
}

#[tokio::test]
async fn relapse_wake_is_not_claimed_before_its_grace() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let t = turn(&f, 80_001).await?;
    let (early, _) = pending_wake(&f, t, 30).await?;
    let (due, _) = pending_wake(&f, t, -1).await?;
    let claimed = f.handle.steerer().claim_due_wakes(50).await?;
    assert!(claimed.contains(&due));
    assert!(
        !claimed.contains(&early),
        "a relapse wake is not pushed before 30 s"
    );
    let context = f.handle.steerer().fetch_due_wake_context(&claimed).await?;
    assert_eq!(context.len(), 1);
    assert!(
        !context[0].stream_open,
        "no stream: the due pass records why it was not pushed"
    );
    Ok(())
}

#[tokio::test]
async fn wrong_on_a_pending_wake_cancels_it() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let t = turn(&f, 80_101).await?;
    let (id, uuid) = pending_wake(&f, t, 60).await?;
    let evaluation: uuid::Uuid =
        sqlx::query_scalar("SELECT uuid FROM steering.evaluations WHERE intervention_uuid = $1")
            .bind(uuid)
            .fetch_one(f.handle.pool())
            .await?;
    let label = f
        .handle
        .steerer()
        .label_steer_evaluation(evaluation, "wrong", Some("it was fine"), 0)
        .await?;
    assert!(label.is_some());
    let cancelled = f.handle.steerer().reject_pending_wake(evaluation).await?;
    assert_eq!(cancelled, vec![id]);
    let state: Option<String> =
        sqlx::query_scalar("SELECT wake_state FROM steering.interventions WHERE id = $1")
            .bind(id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(state.as_deref(), Some("operator_rejected"));
    Ok(())
}

#[tokio::test]
async fn pushed_wakes_in_the_last_hour_are_counted_for_the_cap() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let t = turn(&f, 80_201).await?;
    for _ in 0..3 {
        let (id, _) = pending_wake(&f, t, -1).await?;
        sqlx::query("UPDATE steering.interventions SET wake_state = 'pushed', wake_resolved_at = now() WHERE id = $1")
            .bind(id)
            .execute(f.handle.pool())
            .await?;
    }
    let (fourth, _) = pending_wake(&f, t, -1).await?;
    let claimed = f.handle.steerer().claim_due_wakes(50).await?;
    let context = f.handle.steerer().fetch_due_wake_context(&claimed).await?;
    let row = context.iter().find(|r| r.id == fourth);
    assert_eq!(
        row.map(|r| r.pushed_last_hour),
        Some(3),
        "the fourth wake meets the hourly cap"
    );
    Ok(())
}
