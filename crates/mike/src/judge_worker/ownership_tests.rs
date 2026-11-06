use anyhow::Context;

use super::ownership_test_support::Fixture;

#[tokio::test]
async fn independent_workers_and_stale_selection_make_one_call() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let turns = &f.turns;
    let first = f.judge_turns(&f.persist, turns);
    let competing = async {
        f.model.entered().await?;
        let second = f.judge_turns(&f.peer, turns).await?;
        assert_eq!(second.promoted, 0);
        assert_eq!(f.model.calls(), 1);
        f.model.release();
        anyhow::Ok(())
    };
    let (first, competing) = tokio::join!(first, competing);
    competing?;
    assert_eq!(first?.promoted, 1);
    assert_eq!(f.judge_turns(&f.peer, turns).await?.promoted, 0);
    assert_eq!(f.model.calls(), 1);
    assert_eq!(f.verdict_count().await?, 1);
    f.cleanup().await
}

#[tokio::test]
async fn history_and_immediate_selection_share_turn_ownership() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    f.create_history(300).await?;
    let history = f.history(&f.persist);
    let immediate = async {
        f.model.entered().await?;
        assert_eq!(f.judge_turns(&f.peer, &f.turns).await?.promoted, 0);
        f.model.release();
        anyhow::Ok(())
    };
    let (history, immediate) = tokio::join!(history, immediate);
    immediate?;
    assert_eq!(history, 1);
    assert_eq!(f.model.calls(), 1);
    assert_eq!(f.verdict_count().await?, 1);
    f.cleanup().await
}

#[tokio::test]
async fn history_consumers_share_daily_admission() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(2).await? else {
        return Ok(());
    };
    f.create_history(1).await?;
    let first = f.history(&f.persist);
    let competitor = async {
        f.model.entered().await?;
        assert_eq!(f.history(&f.peer).await, 0);
        f.model.release();
        anyhow::Ok(())
    };
    let (first, competitor) = tokio::join!(first, competitor);
    competitor?;
    assert_eq!(first, 1);
    assert_eq!(f.history(&f.peer).await, 0);
    assert_eq!(f.model.calls(), 1);
    assert_eq!(f.verdict_count().await?, 1);
    let run = f
        .persist
        .active_rejudge_run()
        .await?
        .context("active run")?;
    let progress = f.persist.rejudge_progress(&run.window()).await?;
    assert_eq!(progress.calls_today, 1);
    assert_eq!(progress.remaining, 1);
    f.cleanup().await
}

#[tokio::test]
async fn refreshed_next_evidence_allows_legitimate_rejudgement() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(2).await? else {
        return Ok(());
    };
    f.model.release();
    let first = [f.turns[0]];
    assert_eq!(f.judge_turns(&f.persist, &first).await?.promoted, 1);
    sqlx::query(
        "UPDATE memory.turn_facts f SET next_state = 'resolved',
             next_genuine_request_id = t.start_request_id, next_genuine_at = now()
         FROM memory.operator_turns t WHERE f.operator_turn_id = $1 AND t.id = $2",
    )
    .bind(first[0])
    .bind(f.turns[1])
    .execute(f.persist.pool())
    .await?;
    f.persist.reconcile_judge_queue(f.bundle.id).await?;
    assert_eq!(f.judge_turns(&f.peer, &first).await?.promoted, 1);
    assert_eq!(f.model.calls(), 2);
    assert_eq!(f.verdict_count().await?, 2);
    f.cleanup().await
}

#[tokio::test]
async fn provider_failure_releases_turn_ownership() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    f.model.status(401);
    f.model.release();
    assert!(f.judge_turns(&f.persist, &f.turns).await?.stopped);
    let ownership = f
        .peer
        .try_lock_judge_turn(f.bundle.id, f.turns[0])
        .await?
        .context("released ownership")?;
    ownership.release().await?;
    f.model.status(200);
    assert_eq!(f.judge_turns(&f.peer, &f.turns).await?.promoted, 1);
    assert_eq!(f.model.calls(), 2);
    f.cleanup().await
}

#[tokio::test]
async fn persistence_failure_releases_turn_ownership() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    sqlx::query("ALTER TABLE memory.turn_verdicts ADD CONSTRAINT test_reject_verdict CHECK (false) NOT VALID")
        .execute(f.persist.pool()).await?;
    f.model.release();
    assert_eq!(f.judge_turns(&f.persist, &f.turns).await?.promoted, 0);
    assert_eq!(f.verdict_count().await?, 0);
    sqlx::query("ALTER TABLE memory.turn_verdicts DROP CONSTRAINT test_reject_verdict")
        .execute(f.persist.pool())
        .await?;
    assert_eq!(f.judge_turns(&f.peer, &f.turns).await?.promoted, 1);
    assert_eq!(f.model.calls(), 2);
    f.cleanup().await
}

#[tokio::test]
async fn cancelled_provider_future_releases_turn_ownership() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let mut first = Box::pin(f.judge_turns(&f.persist, &f.turns));
    tokio::select! {
        result = &mut first => anyhow::bail!("provider completed before release: {result:?}"),
        entered = f.model.entered() => entered?,
    }
    drop(first);
    let ownership = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(ownership) = f.peer.try_lock_judge_turn(f.bundle.id, f.turns[0]).await? {
                return anyhow::Ok(ownership);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;
    ownership.release().await?;
    assert_eq!(f.verdict_count().await?, 0);
    f.model.release();
    assert_eq!(f.judge_turns(&f.peer, &f.turns).await?.promoted, 1);
    assert_eq!(f.verdict_count().await?, 1);
    f.cleanup().await
}

#[tokio::test]
async fn refreshed_backoff_prevents_stale_selected_call() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    sqlx::query("UPDATE memory.operator_turns SET judge_next_attempt_at = now() + interval '1 hour' WHERE id = $1")
        .bind(f.turns[0]).execute(f.persist.pool()).await?;
    assert_eq!(f.judge_turns(&f.persist, &f.turns).await?.promoted, 0);
    assert_eq!(f.model.calls(), 0);
    f.cleanup().await
}

#[tokio::test]
async fn provider_wait_over_sixty_seconds_retains_ownership() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let first = f.judge_turns(&f.persist, &f.turns);
    let competitor = async {
        f.model.entered().await?;
        tokio::time::sleep(std::time::Duration::from_secs(61)).await;
        assert_eq!(f.judge_turns(&f.peer, &f.turns).await?.promoted, 0);
        assert_eq!(f.model.calls(), 1);
        f.model.release();
        anyhow::Ok(())
    };
    let (first, competitor) = tokio::join!(first, competitor);
    competitor?;
    assert_eq!(first?.promoted, 1);
    assert_eq!(f.verdict_count().await?, 1);
    f.cleanup().await
}

#[tokio::test]
async fn lost_ownership_cancels_provider_future_without_persisting() -> anyhow::Result<()> {
    let Some(f) = Fixture::new(1).await? else {
        return Ok(());
    };
    let first = f.judge_turns(&f.persist, &f.turns);
    let terminate = async {
        f.model.entered().await?;
        let terminated: Vec<bool> = sqlx::query_scalar(
            "SELECT pg_terminate_backend(l.pid)
             FROM pg_locks l JOIN pg_database d ON d.oid = l.database
             WHERE l.locktype = 'advisory' AND l.granted AND d.datname = current_database()",
        )
        .fetch_all(f.peer.pool())
        .await?;
        assert_eq!(terminated, vec![true]);
        anyhow::Ok(())
    };
    let (first, terminate) = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        tokio::join!(first, terminate)
    })
    .await?;
    terminate?;
    assert_eq!(first?.promoted, 0);
    assert_eq!(f.verdict_count().await?, 0);
    f.model.release();
    assert_eq!(f.judge_turns(&f.peer, &f.turns).await?.promoted, 1);
    assert_eq!(f.model.calls(), 2);
    assert_eq!(f.verdict_count().await?, 1);
    f.cleanup().await
}
