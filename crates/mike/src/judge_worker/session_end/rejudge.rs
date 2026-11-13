use crate::store::persistence::eval_repo::EvalPersist as _;
use std::time::Instant;

use chrono::Duration;

use crate::circuit_breaker::CircuitBreaker;
use crate::dossier::view::types::{AsOf, AsOfMode};
use crate::llm::LlmCallContext;
use crate::logged::ok_or_warn;
use crate::store::persistence::eval_rejudge::{RejudgeFinish, RejudgeRunRow};
use crate::store::persistence::session_end::{RejudgeCandidate, SessionRejudgeWindow};

use super::config::SessionJudgeV3Config;
use super::live::{LivePoll, load_config, verdict_row};
use super::run::{SessionJudgeFailure, SessionSubject, judge_session_v3};

const MAX_PER_POLL: i64 = 10;
const SETTLE_MINUTES: i64 = 30;

async fn active_run(poll: &LivePoll<'_>) -> Option<RejudgeRunRow> {
    let run = ok_or_warn(
        poll.persist.eval().active_rejudge_run().await,
        "session_rejudge.active_run",
    )??;
    (run.role == "session_judge" && run.bundle_id == poll.bundle_id).then_some(run)
}

async fn judge_one(
    poll: &LivePoll<'_>,
    cfg: &SessionJudgeV3Config,
    c: &RejudgeCandidate,
    breaker: &mut CircuitBreaker,
) -> bool {
    let llm = poll.llm.with_call_context(LlmCallContext {
        session_id: Some(c.session_db_id),
        steerer_id: None,
        operator_turn_id: Some(c.end_turn_id),
    });
    let at = c
        .closed_at
        .map_or_else(chrono::Utc::now, |t| t + Duration::minutes(SETTLE_MINUTES));
    let subject = SessionSubject {
        session_db_id: c.session_db_id,
        as_of: AsOf {
            at,
            mode: AsOfMode::Reconstructed,
        },
    };
    match judge_session_v3(poll.persist, &llm, cfg, &subject).await {
        Ok(j) => {
            breaker.record(true, Instant::now());
            let row = verdict_row(&j, cfg, c.session_db_id, "live", Some(poll.bundle_id));
            let stored = ok_or_warn(
                poll.persist.eval().insert_session_verdict_v3(&row).await,
                "session_rejudge.store_verdict",
            );
            poll.record(c.session_db_id, cfg, stored.is_none()).await;
            if stored.is_some() && j.end.is_some() {
                poll.project(vec![c.end_turn_id]).await;
            }
            true
        }
        Err(SessionJudgeFailure::Refused) => false,
        Err(e) => {
            breaker.record(false, Instant::now());
            tracing::warn!(session_id = %c.session_uuid, error = %e, "session rejudge: judging failed");
            poll.record(c.session_db_id, cfg, true).await;
            !breaker.should_skip(Instant::now())
        }
    }
}

pub fn claim_size(cap_left_today: i64) -> i64 {
    cap_left_today.clamp(0, MAX_PER_POLL)
}

pub async fn drain(poll: &LivePoll<'_>, cap: i64, breaker: &mut CircuitBreaker) -> usize {
    drain_run(poll, cap, breaker).await.unwrap_or(0)
}

async fn claim_candidates(
    poll: &LivePoll<'_>,
    run: &RejudgeRunRow,
    window: &SessionRejudgeWindow<'_>,
    cap: i64,
) -> Option<(i64, Vec<RejudgeCandidate>)> {
    let today = ok_or_warn(
        poll.persist
            .eval()
            .session_rejudge_calls_today(window)
            .await,
        "session_rejudge.calls_today",
    )?;
    let size = claim_size(cap.min(i64::from(run.daily_call_cap)) - today);
    let candidates = ok_or_warn(
        poll.persist
            .eval()
            .session_rejudge_candidates(window, size.max(1))
            .await,
        "session_rejudge.claim",
    )?;
    Some((size, candidates))
}

async fn drain_run(poll: &LivePoll<'_>, cap: i64, breaker: &mut CircuitBreaker) -> Option<usize> {
    let run = active_run(poll).await?;
    let mut ownership = ok_or_warn(
        poll.persist.try_lock_judge_history(run.id).await,
        "session_rejudge.acquire_ownership",
    )??;
    let outcome = tokio::select! {
        biased;
        _ = crate::judge_worker::ownership::lost(&mut ownership) => None,
        outcome = drain_owned_run(poll, cap, breaker, run.id) => outcome,
    };
    ok_or_warn(
        ownership.release().await,
        "session_rejudge.release_ownership",
    )?;
    outcome
}

async fn started(poll: &LivePoll<'_>, run: &RejudgeRunRow) -> bool {
    run.status == "running"
        || (run.status == "queued"
            && ok_or_warn(
                poll.persist.eval().start_rejudge_run(run.id).await,
                "session_rejudge.start_run",
            )
            .unwrap_or(false))
}

async fn drain_owned_run(
    poll: &LivePoll<'_>,
    cap: i64,
    breaker: &mut CircuitBreaker,
    run_id: i64,
) -> Option<usize> {
    let run = active_run(poll).await?;
    if run.id != run_id || !started(poll, &run).await {
        return None;
    }
    let cfg = load_config(poll.persist, poll.bundle_id).await?;
    let window = SessionRejudgeWindow {
        bundle_id: poll.bundle_id,
        since: run.window_since,
        until: run.window_until,
        judge_model: &cfg.model,
    };
    let (size, candidates) = claim_candidates(poll, &run, &window, cap).await?;
    if candidates.is_empty() {
        ok_or_warn(
            poll.persist
                .eval()
                .finish_rejudge_run(run.id, RejudgeFinish::Done)
                .await,
            "session_rejudge.finish_run",
        );
        return None;
    }
    let mut judged = 0;
    for c in candidates.iter().take(usize::try_from(size).unwrap_or(0)) {
        if !judge_one(poll, &cfg, c, breaker).await {
            break;
        }
        judged += 1;
    }
    Some(judged)
}

#[cfg(test)]
mod tests {
    use super::claim_size;

    #[test]
    fn a_poll_never_claims_past_the_cap_left() {
        assert_eq!(claim_size(0), 0);
        assert_eq!(claim_size(4), 4);
        assert_eq!(claim_size(200), 10);
    }
}

#[cfg(test)]
#[path = "rejudge_tests.rs"]
mod ownership_tests;
