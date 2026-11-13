use chrono::{Duration, Utc};
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::operator_turns::LiveClaim;
use mike::store::persistence::operator_turns::projection::ProjectionMode;
use mike::store::persistence::turn_verdicts::VerdictMode;

use super::promotion_support::{
    bundle, closed_turn, facts, params, projected, resolved_verdict as verdict,
};
use crate::turn_facts::{Fixture, fixture};

fn claim(bundle_id: i64) -> LiveClaim {
    LiveClaim {
        bundle_id,
        since: Utc::now() - Duration::days(7),
        limit: 50,
    }
}

async fn eligible(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    let turn = closed_turn(f, start).await?;
    facts(f, turn, "genuine", "resolved").await?;
    Ok(turn)
}

#[tokio::test]
async fn live_claim_selects_genuine_resolved_turns_without_a_live_verdict() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let first = eligible(&f, 22_001).await?;
    let judged = eligible(&f, 22_002).await?;
    let pending = closed_turn(&f, 22_003).await?;
    facts(&f, pending, "genuine", "pending").await?;
    let machine = closed_turn(&f, 22_004).await?;
    facts(&f, machine, "background-event", "resolved").await?;
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            judged,
            VerdictMode::Live,
            b,
            ("accepted", "delivered"),
            0.9,
        ))
        .await?;
    let ids = f.handle.claim_live_candidates(claim(b)).await?;
    assert_eq!(ids, vec![first]);
    f.handle.release_judge_leases(&ids).await?;
    let per_session = f
        .handle
        .claim_live_candidates_for_session(f.session_id, b, 5)
        .await?;
    assert_eq!(per_session, vec![first]);
    Ok(())
}

#[tokio::test]
async fn live_claim_skips_turn_opened_by_agent_id_request_and_permanent_failures()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let agent = eligible(&f, 22_101).await?;
    sqlx::query("UPDATE requests.requests SET claude_code_agent_id = 'agent-1' WHERE id = 22101")
        .execute(f.handle.pool())
        .await?;
    let failed = eligible(&f, 22_102).await?;
    sqlx::query(
        "UPDATE memory.operator_turns SET judge_failure_reason = 'permanent' WHERE id = $1",
    )
    .bind(failed)
    .execute(f.handle.pool())
    .await?;
    let ids = f.handle.claim_live_candidates(claim(b)).await?;
    assert!(!ids.contains(&agent));
    assert!(!ids.contains(&failed));
    Ok(())
}

#[tokio::test]
async fn mark_live_judge_failed_backs_off_and_skips_judged_turns() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = eligible(&f, 22_201).await?;
    let failure = mike::store::persistence::operator_turns::LiveFailure {
        turn_id: turn,
        reason: "transient_exhausted",
        bundle_id: b,
        backoff_base_secs: 60,
        backoff_max_secs: 3600,
    };
    assert!(f.handle.mark_live_judge_failed(failure).await?);
    let ids = f.handle.claim_live_candidates(claim(b)).await?;
    assert!(ids.is_empty(), "a backed-off turn waits");
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            turn,
            VerdictMode::Live,
            b,
            ("accepted", "delivered"),
            0.9,
        ))
        .await?;
    assert!(!f.handle.mark_live_judge_failed(failure).await?);
    Ok(())
}

#[tokio::test]
async fn clear_transient_judge_backoff_makes_a_backed_off_turn_claimable() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let transient = eligible(&f, 22_401).await?;
    let permanent = eligible(&f, 22_402).await?;
    let failure = |turn_id, reason| mike::store::persistence::operator_turns::LiveFailure {
        turn_id,
        reason,
        bundle_id: b,
        backoff_base_secs: 3600,
        backoff_max_secs: 14_400,
    };
    f.handle
        .mark_live_judge_failed(failure(transient, "transient_exhausted"))
        .await?;
    f.handle
        .mark_live_judge_failed(failure(permanent, "permanent"))
        .await?;
    assert!(
        !f.handle
            .claim_live_candidates(claim(b))
            .await?
            .contains(&transient)
    );
    assert!(f.handle.clear_transient_judge_backoff().await? >= 1);
    let ids = f.handle.claim_live_candidates(claim(b)).await?;
    assert!(ids.contains(&transient));
    assert!(!ids.contains(&permanent));
    Ok(())
}

#[tokio::test]
async fn reproject_final_turns_resets_ineligible_and_leaves_pending() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let machine = closed_turn(&f, 22_301).await?;
    facts(&f, machine, "background-event", "resolved").await?;
    let pending = closed_turn(&f, 22_302).await?;
    facts(&f, pending, "genuine", "pending").await?;
    let rows = f
        .handle
        .reproject_final_turns(&params(b, ProjectionMode::Apply), 7)
        .await?;
    assert_eq!(rows, vec![(machine, "unknown".to_owned())]);
    assert_eq!(
        projected(&f, machine).await?.outcome_source.as_deref(),
        Some("no_verdict")
    );
    assert_eq!(projected(&f, pending).await?.outcome_source, None);
    let backlog = f
        .handle
        .live_backlog(b, Utc::now() + Duration::hours(1))
        .await?;
    assert_eq!(
        backlog.eligible_without_verdict, 0,
        "a pending turn is not yet eligible"
    );
    Ok(())
}

#[tokio::test]
async fn live_claim_dated_returns_closed_at_matching_the_uncapped_backlog() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let first = eligible(&f, 22_501).await?;
    let second = eligible(&f, 22_502).await?;

    let backlog = f
        .handle
        .live_backlog(b, Utc::now() + Duration::hours(1))
        .await?;
    assert_eq!(backlog.eligible_without_verdict, 2);
    assert!(!backlog.capped);

    let dated = f.handle.claim_live_candidates_dated(claim(b)).await?;
    let mut ids: Vec<i64> = dated.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(ids, expected);
    assert!(dated.windows(2).all(|pair| pair[0].1 <= pair[1].1));
    assert_eq!(backlog.oldest_closed_at, dated.first().map(|(_, at)| *at));
    Ok(())
}
