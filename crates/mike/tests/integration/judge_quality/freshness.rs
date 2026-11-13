use chrono::{Duration, Utc};
use mike::store::persistence::eval_rejudge::RejudgeWindow;
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::operator_turns::projection::ProjectionMode;
use mike::store::persistence::operator_turns::{LiveClaim, LiveFailure};
use mike::store::persistence::turn_verdicts::VerdictMode;

use super::promotion_support::{bundle, closed_turn, facts, params, projected, verdict};
use crate::turn_facts::fixture;

#[tokio::test]
async fn changed_followup_reclaims_and_reprojects_without_losing_history() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 29_001).await?;
    facts(&f, turn, "genuine", "resolved").await?;
    let mut old = verdict(turn, VerdictMode::Live, b, ("corrected", "partial"), 0.9);
    old.next_genuine_request_id = Some(2);
    let old_id = f.handle.eval().insert_turn_verdict(old).await?;
    let p = params(b, ProjectionMode::Apply);
    f.handle.project_turn_outcomes(&[turn], &p).await?;
    assert_eq!(projected(&f, turn).await?.live_verdict_id, Some(old_id));
    let claim = LiveClaim {
        bundle_id: b,
        since: Utc::now() - Duration::days(7),
        limit: 10,
    };
    assert!(f.handle.claim_live_candidates(claim).await?.is_empty());
    sqlx::query(
        "UPDATE memory.turn_facts SET next_genuine_request_id = 3 WHERE operator_turn_id = $1",
    )
    .bind(turn)
    .execute(f.handle.pool())
    .await?;
    f.handle.reconcile_judge_queue(b).await?;
    assert_eq!(f.handle.claim_live_candidates(claim).await?, vec![turn]);
    f.handle.release_judge_leases(&[turn]).await?;
    let window = RejudgeWindow {
        bundle_id: b,
        since: claim.since,
        until: Utc::now() + Duration::hours(1),
    };
    assert_eq!(
        f.handle
            .eval()
            .claim_rejudge_candidates(&window, 10)
            .await?,
        vec![turn]
    );
    f.handle.reproject_final_turns(&p, 7).await?;
    assert_eq!(
        projected(&f, turn).await?.outcome_source.as_deref(),
        Some("no_verdict")
    );
    assert!(f.handle.reproject_final_turns(&p, 7).await?.is_empty());
    let failure = LiveFailure {
        turn_id: turn,
        reason: "transient_exhausted",
        bundle_id: b,
        backoff_base_secs: 60,
        backoff_max_secs: 3600,
    };
    assert!(f.handle.mark_live_judge_failed(failure).await?);
    assert!(f.handle.claim_live_candidates(claim).await?.is_empty());
    let mut fresh = verdict(turn, VerdictMode::Live, b, ("accepted", "shipped"), 0.9);
    fresh.next_genuine_request_id = Some(3);
    let (fresh_id, _) = f
        .handle
        .eval()
        .insert_live_verdict_and_promote(fresh, &p)
        .await?;
    assert_eq!(projected(&f, turn).await?.live_verdict_id, Some(fresh_id));
    assert!(!f.handle.mark_live_judge_failed(failure).await?);
    assert!(f.handle.claim_live_candidates(claim).await?.is_empty());
    let history: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.turn_verdicts WHERE operator_turn_id = $1")
            .bind(turn)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(history, 2);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reclassified_machine_turn_resets_existing_judge_projection() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 29_101).await?;
    facts(&f, turn, "genuine", "none_session_settled").await?;
    let p = params(b, ProjectionMode::Apply);
    f.handle
        .eval()
        .insert_live_verdict_and_promote(
            verdict(turn, VerdictMode::Live, b, ("no_response", "shipped"), 0.9),
            &p,
        )
        .await?;
    assert_eq!(
        projected(&f, turn).await?.outcome_source.as_deref(),
        Some("judge")
    );
    facts(&f, turn, "background-event", "none_session_settled").await?;
    f.handle.reproject_final_turns(&p, 7).await?;
    assert_eq!(projected(&f, turn).await?.outcome, "unknown");
    assert_eq!(
        projected(&f, turn).await?.outcome_source.as_deref(),
        Some("no_verdict")
    );
    assert!(f.handle.reproject_final_turns(&p, 7).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}
