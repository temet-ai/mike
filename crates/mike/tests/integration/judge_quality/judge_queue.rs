use chrono::{DateTime, Duration, Utc};
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::eval_rejudge::{NewRejudgeRun, RejudgeWindow};
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::operator_turns::projection::ProjectionMode;
use mike::store::persistence::operator_turns::{LiveClaim, LiveFailure};
use mike::store::persistence::turn_verdicts::VerdictMode;

use super::promotion_support::{bundle, closed_turn, facts, params, verdict};
use crate::turn_facts::{Fixture, fixture, resolve, sample_row};

fn claim(bundle_id: i64) -> LiveClaim {
    LiveClaim {
        bundle_id,
        since: Utc::now() - Duration::days(7),
        limit: 10,
    }
}

async fn queued_at(f: &Fixture, turn: i64) -> anyhow::Result<Option<DateTime<Utc>>> {
    Ok(
        sqlx::query_scalar("SELECT judge_queued_at FROM memory.operator_turns WHERE id = $1")
            .bind(turn)
            .fetch_one(f.handle.pool())
            .await?,
    )
}

async fn facts_with_next(f: &Fixture, turn: i64, next_genuine: i64) -> anyhow::Result<()> {
    let mut row = sample_row(turn, f.session_id, Utc::now());
    resolve(&mut row);
    row.next_genuine_request_id = Some(next_genuine);
    f.handle.dossier().upsert_turn_facts(&[row]).await?;
    Ok(())
}

#[tokio::test]
async fn facts_upsert_moves_the_queue_column_with_eligibility() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 31_001).await?;
    facts(&f, turn, "genuine", "pending").await?;
    assert!(
        queued_at(&f, turn).await?.is_none(),
        "an unsettled follow-up is not judge-eligible"
    );
    facts(&f, turn, "genuine", "resolved").await?;
    assert!(
        queued_at(&f, turn).await?.is_some(),
        "a settled genuine turn joins the queue"
    );
    facts(&f, turn, "background-event", "resolved").await?;
    assert!(
        queued_at(&f, turn).await?.is_none(),
        "a reclassified machine prompt leaves the queue"
    );
    Ok(())
}

#[tokio::test]
async fn a_matching_live_verdict_dequeues_and_a_changed_followup_requeues() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 31_101).await?;
    facts(&f, turn, "genuine", "resolved").await?;
    let mut live = verdict(turn, VerdictMode::Live, b, ("accepted", "shipped"), 0.9);
    live.next_genuine_request_id = Some(2);
    f.handle
        .eval()
        .insert_live_verdict_and_promote(live, &params(b, ProjectionMode::Apply))
        .await?;
    assert!(
        queued_at(&f, turn).await?.is_none(),
        "a live verdict matching the facts settles the turn"
    );
    assert!(f.handle.claim_live_candidates(claim(b)).await?.is_empty());

    facts_with_next(&f, turn, 3).await?;
    assert!(
        queued_at(&f, turn).await?.is_some(),
        "a new follow-up makes the verdict stale"
    );
    assert_eq!(f.handle.claim_live_candidates(claim(b)).await?, vec![turn]);
    f.handle.release_judge_leases(&[turn]).await?;

    let mut shadow = verdict(turn, VerdictMode::Shadow, b, ("accepted", "shipped"), 0.9);
    shadow.next_genuine_request_id = Some(3);
    f.handle.eval().insert_turn_verdict(shadow).await?;
    assert!(
        queued_at(&f, turn).await?.is_some(),
        "a shadow verdict does not settle the turn"
    );
    Ok(())
}

#[tokio::test]
async fn a_permanent_failure_dequeues_and_a_rejudge_run_requeues() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 31_201).await?;
    facts(&f, turn, "genuine", "resolved").await?;
    let transient = LiveFailure {
        turn_id: turn,
        reason: "transient_exhausted",
        bundle_id: b,
        backoff_base_secs: 60,
        backoff_max_secs: 3_600,
    };
    assert!(f.handle.mark_live_judge_failed(transient).await?);
    assert!(
        queued_at(&f, turn).await?.is_some(),
        "a transient failure keeps the turn queued behind its backoff"
    );
    let permanent = LiveFailure {
        reason: "permanent",
        ..transient
    };
    assert!(f.handle.mark_live_judge_failed(permanent).await?);
    assert!(
        queued_at(&f, turn).await?.is_none(),
        "a permanent failure leaves the queue"
    );

    let window = RejudgeWindow {
        bundle_id: b,
        since: Utc::now() - Duration::days(7),
        until: Utc::now() + Duration::hours(1),
    };
    let run = NewRejudgeRun {
        window,
        daily_call_cap: 10,
        estimated_calls: 1,
        estimated_usd: None,
        created_by: "judge_queue_test",
        role: "turn_judge",
    };
    f.handle.eval().create_rejudge_run(&run).await?;
    assert!(
        queued_at(&f, turn).await?.is_some(),
        "a re-judge run requeues the turns it resets"
    );
    assert_eq!(
        f.handle
            .eval()
            .claim_rejudge_candidates(&window, 10)
            .await?,
        vec![turn]
    );
    Ok(())
}

#[tokio::test]
async fn reconcile_repairs_the_column_and_the_claim_reads_only_the_queue() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let eligible = closed_turn(&f, 31_301).await?;
    facts(&f, eligible, "genuine", "resolved").await?;
    let machine = closed_turn(&f, 31_302).await?;
    facts(&f, machine, "background-event", "resolved").await?;
    sqlx::query(
        "UPDATE memory.operator_turns
            SET judge_queued_at = CASE WHEN id = $1 THEN NULL ELSE now() END
          WHERE id IN ($1, $2)",
    )
    .bind(eligible)
    .bind(machine)
    .execute(f.handle.pool())
    .await?;
    assert!(
        f.handle.claim_live_candidates(claim(b)).await?.is_empty(),
        "the claim starts from the queue column"
    );

    let done = f.handle.reconcile_judge_queue(b).await?;
    assert_eq!((done.queued, done.dequeued), (1, 1));
    assert!(queued_at(&f, eligible).await?.is_some());
    assert!(queued_at(&f, machine).await?.is_none());
    assert_eq!(
        f.handle.claim_live_candidates(claim(b)).await?,
        vec![eligible]
    );
    Ok(())
}
