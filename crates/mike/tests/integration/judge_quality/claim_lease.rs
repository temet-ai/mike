use std::collections::HashSet;
use std::sync::Arc;

use chrono::{Duration, Utc};
use mike::store::persistence::eval_rejudge::RejudgeWindow;
use mike::store::persistence::operator_turns::{LiveClaim, LiveFailure};
use tokio::sync::Barrier;

use super::promotion_support::{bundle, closed_turn, facts};
use crate::turn_facts::Fixture;

const TURNS: usize = 24;
const CLAIMERS: usize = 6;

fn live_claim(bundle_id: i64, limit: i64) -> LiveClaim {
    LiveClaim {
        bundle_id,
        since: Utc::now() - Duration::days(7),
        limit,
    }
}

async fn eligible_turns(f: &Fixture, count: usize) -> anyhow::Result<Vec<i64>> {
    let mut turns = Vec::with_capacity(count);
    for n in 0..count {
        let turn = closed_turn(f, 40_000 + i64::try_from(n)?).await?;
        facts(f, turn, "genuine", "resolved").await?;
        turns.push(turn);
    }
    Ok(turns)
}

async fn lease_of(f: &Fixture, turn: i64) -> anyhow::Result<Option<chrono::DateTime<Utc>>> {
    Ok(
        sqlx::query_scalar("SELECT judge_claimed_until FROM memory.operator_turns WHERE id = $1")
            .bind(turn)
            .fetch_one(f.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn concurrent_claims_across_all_three_paths_never_return_the_same_turn() -> anyhow::Result<()>
{
    let Some(f) = crate::turn_facts::fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turns = eligible_turns(&f, TURNS).await?;
    let window = RejudgeWindow {
        bundle_id: b,
        since: Utc::now() - Duration::days(7),
        until: Utc::now() + Duration::hours(1),
    };
    let barrier = Arc::new(Barrier::new(CLAIMERS));
    let mut tasks = Vec::new();
    for n in 0..CLAIMERS {
        let handle = f.handle.clone();
        let barrier = barrier.clone();
        let session_id = f.session_id;
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            match n % 3 {
                0 => handle.claim_live_candidates(live_claim(b, 10)).await,
                1 => {
                    handle
                        .claim_live_candidates_for_session(session_id, b, 10)
                        .await
                }
                _ => handle.claim_rejudge_candidates(&window, 10).await,
            }
        }));
    }
    let mut claimed = Vec::new();
    for task in tasks {
        claimed.extend(task.await??);
    }
    let distinct: HashSet<i64> = claimed.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        claimed.len(),
        "a turn was claimed by two workers: {claimed:?}"
    );
    assert_eq!(
        distinct,
        turns.iter().copied().collect::<HashSet<_>>(),
        "every eligible turn is claimed exactly once"
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_claimed_turn_stays_out_of_every_claim_until_released_or_expired() -> anyhow::Result<()> {
    let Some(f) = crate::turn_facts::fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turns = eligible_turns(&f, 1).await?;
    let turn = turns[0];
    let window = RejudgeWindow {
        bundle_id: b,
        since: Utc::now() - Duration::days(7),
        until: Utc::now() + Duration::hours(1),
    };

    assert_eq!(
        f.handle.claim_live_candidates(live_claim(b, 5)).await?,
        vec![turn]
    );
    assert!(lease_of(&f, turn).await?.is_some_and(|t| t > Utc::now()));
    assert!(
        f.handle
            .claim_live_candidates(live_claim(b, 5))
            .await?
            .is_empty()
    );
    assert!(
        f.handle
            .claim_live_candidates_for_session(f.session_id, b, 5)
            .await?
            .is_empty()
    );
    assert!(
        f.handle
            .claim_rejudge_candidates(&window, 5)
            .await?
            .is_empty()
    );

    assert_eq!(f.handle.release_judge_leases(&[turn]).await?, 1);
    assert_eq!(lease_of(&f, turn).await?, None);
    assert_eq!(f.handle.release_judge_leases(&[turn]).await?, 0);
    assert_eq!(
        f.handle
            .claim_live_candidates_for_session(f.session_id, b, 5)
            .await?,
        vec![turn]
    );

    sqlx::query(
        "UPDATE memory.operator_turns SET judge_claimed_until = now() - interval '1 second' WHERE id = $1",
    )
    .bind(turn)
    .execute(f.handle.pool())
    .await?;
    assert_eq!(
        f.handle.claim_rejudge_candidates(&window, 5).await?,
        vec![turn],
        "a lease whose worker died expires on its own"
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn renewing_a_lease_before_it_runs_out_keeps_other_claims_away() -> anyhow::Result<()> {
    let Some(f) = crate::turn_facts::fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = eligible_turns(&f, 1).await?[0];
    assert_eq!(
        f.handle.claim_live_candidates(live_claim(b, 5)).await?,
        vec![turn]
    );
    sqlx::query(
        "UPDATE memory.operator_turns SET judge_claimed_until = now() + interval '1 second' WHERE id = $1",
    )
    .bind(turn)
    .execute(f.handle.pool())
    .await?;
    assert_eq!(f.handle.renew_judge_leases(&[turn]).await?, 1);
    let renewed = lease_of(&f, turn).await?;
    assert!(renewed.is_some_and(|t| t > Utc::now() + Duration::minutes(5)));
    assert!(
        f.handle
            .claim_live_candidates_for_session(f.session_id, b, 5)
            .await?
            .is_empty()
    );
    f.handle.release_judge_leases(&[turn]).await?;
    assert_eq!(f.handle.renew_judge_leases(&[turn]).await?, 0);
    assert_eq!(lease_of(&f, turn).await?, None);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn releasing_a_lease_keeps_the_failure_backoff() -> anyhow::Result<()> {
    let Some(f) = crate::turn_facts::fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = eligible_turns(&f, 1).await?[0];
    assert_eq!(
        f.handle.claim_live_candidates(live_claim(b, 5)).await?,
        vec![turn]
    );
    let failure = LiveFailure {
        turn_id: turn,
        reason: "transient_exhausted",
        bundle_id: b,
        backoff_base_secs: 60,
        backoff_max_secs: 3600,
    };
    assert!(f.handle.mark_live_judge_failed(failure).await?);
    f.handle.release_judge_leases(&[turn]).await?;
    assert_eq!(lease_of(&f, turn).await?, None);
    let backoff: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT judge_next_attempt_at FROM memory.operator_turns WHERE id = $1")
            .bind(turn)
            .fetch_one(f.handle.pool())
            .await?;
    assert!(backoff.is_some_and(|t| t > Utc::now()));
    assert!(
        f.handle
            .claim_live_candidates(live_claim(b, 5))
            .await?
            .is_empty(),
        "the backoff still holds the turn after its lease is released"
    );
    f.db.cleanup().await?;
    Ok(())
}
