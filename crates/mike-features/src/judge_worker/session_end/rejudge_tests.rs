use crate::store::persistence::eval_repo::EvalPersist as _;
use anyhow::Context;
use chrono::Utc;
use serde_json::json;

use crate::analysis::EmbeddingService;
use crate::analysis::embeddings::EmbeddingConfig;
use crate::judge_worker::ownership_test_support::{Fixture, breaker};
use crate::store::persistence::eval_rejudge::{NewRejudgeRun, RejudgeFinish, RejudgeWindow};

use super::super::live::LivePoll;

async fn session_run(f: &Fixture) -> anyhow::Result<(i64, i64)> {
    let bundle = f
        .persist
        .eval()
        .create_candidate_bundle("session_judge", "", &json!({}))
        .await?;
    let id = f
        .persist
        .eval()
        .create_rejudge_run(&NewRejudgeRun {
            window: RejudgeWindow {
                bundle_id: bundle,
                since: Utc::now() - chrono::Duration::days(2),
                until: Utc::now(),
            },
            daily_call_cap: 200,
            estimated_calls: 1,
            estimated_usd: None,
            created_by: "session-ownership-test",
            role: "session_judge",
        })
        .await?;
    Ok((id, bundle))
}

fn poll<'a>(f: &'a Fixture, embedding: &'a EmbeddingService, bundle: i64) -> LivePoll<'a> {
    LivePoll {
        persist: &f.peer,
        llm: &f.llm,
        embedding,
        bundle_id: bundle,
        lookback_hours: 24,
        max_attempts_per_day: 5,
        batch: 10,
        turn_judge_live_bundle_id: f.bundle.id,
        turn_confidence_threshold: 0.5,
        jev: None,
    }
}

#[tokio::test]
async fn competing_session_history_owner_prevents_admission() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let (id, bundle) = session_run(&f).await?;
    let owner = f
        .persist
        .try_lock_judge_history(id)
        .await?
        .context("first history owner")?;
    let embedding = EmbeddingService::new(&EmbeddingConfig::default());
    assert_eq!(
        super::drain(&poll(&f, &embedding, bundle), 200, &mut breaker()).await,
        0
    );
    let run = f
        .persist
        .eval()
        .active_rejudge_run()
        .await?
        .context("queued session run")?;
    assert_eq!(run.id, id);
    assert_eq!(run.status, "queued");
    assert_eq!(f.model.calls(), 0);
    owner.release().await?;
    f.cleanup().await
}

#[tokio::test]
async fn cancelled_queued_session_history_does_not_start() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let (id, bundle) = session_run(&f).await?;
    let run = f
        .persist
        .eval()
        .active_rejudge_run()
        .await?
        .context("queued session run")?;
    f.persist
        .eval()
        .finish_rejudge_run(id, RejudgeFinish::Cancelled)
        .await?;
    let embedding = EmbeddingService::new(&EmbeddingConfig::default());
    assert!(!super::started(&poll(&f, &embedding, bundle), &run).await);
    assert_eq!(f.model.calls(), 0);
    f.cleanup().await
}
