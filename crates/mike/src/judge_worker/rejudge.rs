use crate::store::persistence::eval_repo::EvalPersist as _;
use std::time::Instant;

use crate::circuit_breaker::CircuitBreaker;
use crate::logged::ok_or_warn;
use crate::store::persistence::eval_rejudge::{RejudgeFinish, RejudgeRunRow, RejudgeWindow};
use crate::store::persistence::operator_turns::projection::ProjectionMode;

use super::live_judge::{LiveJudge, judge_and_promote};
use super::turn_sweep::reserve_claim_batch;

const MAX_CLAIM_PER_TICK: i64 = 10;
const INELIGIBLE_PAGE: i64 = 500;
const INELIGIBLE_PAGES_PER_TICK: usize = 10;

pub(crate) fn claim_size(cap_left_today: i64) -> i64 {
    cap_left_today.clamp(0, MAX_CLAIM_PER_TICK)
}

async fn reproject_ineligible(judge: &LiveJudge<'_>, window: &RejudgeWindow) -> usize {
    let params = judge.params(ProjectionMode::ResetWithoutVerdict);
    let mut after = 0_i64;
    let mut total = 0usize;
    for _ in 0..INELIGIBLE_PAGES_PER_TICK {
        let Some(ids) = ok_or_warn(
            judge
                .persist
                .eval()
                .rejudge_ineligible_turn_ids(window, after, INELIGIBLE_PAGE)
                .await,
            "rejudge.ineligible_page",
        ) else {
            return total;
        };
        let Some(last) = ids.last().copied() else {
            return total;
        };
        after = last;
        let Some(rows) = ok_or_warn(
            judge.persist.project_turn_outcomes(&ids, &params).await,
            "rejudge.ineligible_reprojection",
        ) else {
            return total;
        };
        total += rows.len();
    }
    total
}

async fn started(judge: &LiveJudge<'_>, run: &RejudgeRunRow) -> bool {
    run.status == "running"
        || (run.status == "queued"
            && ok_or_warn(
                judge.persist.eval().start_rejudge_run(run.id).await,
                "rejudge.start_run",
            )
            .unwrap_or(false))
}

async fn finish_if_done(judge: &LiveJudge<'_>, run: &RejudgeRunRow) -> Option<i64> {
    let progress = ok_or_warn(
        judge.persist.eval().rejudge_progress(&run.window()).await,
        "rejudge.progress",
    )?;
    if progress.remaining > 0 {
        return Some(progress.cap_left_today(run.daily_call_cap));
    }
    ok_or_warn(
        judge
            .persist
            .eval()
            .finish_rejudge_run(run.id, RejudgeFinish::Done)
            .await,
        "rejudge.finish_run",
    );
    tracing::info!(
        run_id = run.id,
        judged = progress.judged,
        "rejudge: run done"
    );
    None
}

async fn active_run(judge: &LiveJudge<'_>) -> Option<RejudgeRunRow> {
    let run = ok_or_warn(
        judge.persist.eval().active_rejudge_run().await,
        "rejudge.active_run",
    )??;
    if run.bundle_id == judge.bundle.id && run.role == "turn_judge" {
        return Some(run);
    }
    tracing::warn!(
        run_id = run.id,
        run_bundle = run.bundle_id,
        live_bundle = judge.bundle.id,
        "rejudge: the active run names another bundle; waiting"
    );
    None
}

pub(crate) async fn drain(judge: &LiveJudge<'_>, breaker: &mut CircuitBreaker) -> usize {
    drain_run(judge, breaker).await.unwrap_or(0)
}

async fn drain_run(judge: &LiveJudge<'_>, breaker: &mut CircuitBreaker) -> Option<usize> {
    let run = active_run(judge).await?;
    let mut ownership = ok_or_warn(
        judge.persist.try_lock_judge_history(run.id).await,
        "rejudge.acquire_ownership",
    )??;
    let outcome = tokio::select! {
        biased;
        _ = crate::judge_worker::ownership::lost(&mut ownership) => None,
        outcome = drain_owned_run(judge, breaker, run.id) => outcome,
    };
    ok_or_warn(ownership.release().await, "rejudge.release_ownership")?;
    outcome
}

async fn drain_owned_run(
    judge: &LiveJudge<'_>,
    breaker: &mut CircuitBreaker,
    run_id: i64,
) -> Option<usize> {
    let run = active_run(judge).await?;
    if run.id != run_id || !started(judge, &run).await {
        return None;
    }
    reproject_ineligible(judge, &run.window()).await;
    let size = claim_size(finish_if_done(judge, &run).await?);
    if size == 0 || breaker.should_skip(Instant::now()) {
        return None;
    }
    let reservation = reserve_claim_batch(judge.llm, false, size).ok()?;
    let ids = ok_or_warn(
        judge
            .persist
            .eval()
            .claim_rejudge_candidates(&run.window(), i64::from(reservation.granted()))
            .await,
        "rejudge.claim",
    )?;
    Some(
        judge_and_promote(judge, &reservation, breaker, &ids)
            .await
            .promoted,
    )
}

#[cfg(test)]
mod tests {
    use super::claim_size;
    use crate::store::persistence::eval_repo::EvalPersist as _;

    #[test]
    fn claim_size_is_capped_by_daily_cap_left() {
        assert_eq!(claim_size(0), 0);
        assert_eq!(claim_size(3), 3);
        assert_eq!(claim_size(250), 10);
        assert_eq!(claim_size(-4), 0);
    }
    #[tokio::test]
    async fn cancelled_queued_run_does_not_start() -> anyhow::Result<()> {
        use crate::judge_worker::live_shared::LiveJudgeShared;
        use crate::judge_worker::ownership_test_support::Fixture;
        use crate::store::persistence::eval_rejudge::RejudgeFinish;
        use anyhow::Context;

        let Some(f) = Fixture::new(1).await? else {
            return Ok(());
        };
        let id = f.create_history(1).await?;
        let run = f
            .persist
            .eval()
            .active_rejudge_run()
            .await?
            .context("queued run")?;
        f.persist
            .eval()
            .finish_rejudge_run(id, RejudgeFinish::Cancelled)
            .await?;
        let shared = LiveJudgeShared::default();
        assert!(!super::started(&f.judge(&f.persist, &shared), &run).await);
        assert_eq!(f.model.calls(), 0);
        f.cleanup().await
    }
}
