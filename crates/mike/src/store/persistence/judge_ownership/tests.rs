use std::time::Duration;

use super::*;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

async fn handles() -> anyhow::Result<Option<(TestDbHandle, PersistHandle, PersistHandle)>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let first = PersistHandle::connect_existing(&db.database_url).await?;
    let second = PersistHandle::connect_existing(&db.database_url).await?;
    Ok(Some((db, first, second)))
}

async fn reacquire(handle: &PersistHandle) -> anyhow::Result<JudgeOwnership> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(owner) = handle.try_lock_judge_turn(14, 42).await? {
                return Ok::<_, PersistError>(owner);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
    .map_err(Into::into)
}

#[tokio::test]
async fn independent_handles_race_only_one_owns_turn_until_release() -> anyhow::Result<()> {
    let Some((db, first, second)) = handles().await? else {
        return Ok(());
    };
    let (a, b) = tokio::join!(
        first.try_lock_judge_turn(14, 42),
        second.try_lock_judge_turn(14, 42)
    );
    let (a, b) = (a?, b?);
    assert_ne!(a.is_some(), b.is_some());
    let owner = a.or(b).ok_or_else(|| anyhow::anyhow!("no winner"))?;
    assert!(first.try_lock_judge_turn(14, 42).await?.is_none());
    assert!(second.try_lock_judge_turn(14, 42).await?.is_none());
    owner.release().await?;
    reacquire(&second).await?.release().await?;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capacity_is_shared_by_clones_without_using_query_pool_slots() -> anyhow::Result<()> {
    let Some((db, first, second)) = handles().await? else {
        return Ok(());
    };
    let clone = first.clone();
    let mut query_connections = Vec::new();
    for _ in 0..8 {
        query_connections.push(first.pool.acquire().await?);
    }
    let a = first.try_lock_judge_turn(14, 42).await?;
    let b = clone.try_lock_judge_turn(15, 42).await?;
    assert!(a.is_some() && b.is_some());
    assert!(first.try_lock_judge_turn(14, 43).await?.is_none());
    let history = first.try_lock_judge_history(42).await?;
    assert!(history.is_some());
    assert!(clone.try_lock_judge_history(43).await?.is_none());
    assert!(second.try_lock_judge_history(42).await?.is_none());
    for owner in [a, b, history].into_iter().flatten() {
        owner.release().await?;
    }
    assert_eq!(first.judge_ownership_capacity.turns.available_permits(), 2);
    assert_eq!(
        first.judge_ownership_capacity.history.available_permits(),
        1
    );
    drop(query_connections);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn cancellation_closes_ownership_connection_and_returns_capacity() -> anyhow::Result<()> {
    let Some((db, first, second)) = handles().await? else {
        return Ok(());
    };
    let clone = first.clone();
    let (ready, wait) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _owner = clone
            .try_lock_judge_turn(14, 42)
            .await?
            .ok_or_else(|| anyhow::anyhow!("lock unavailable"))?;
        let _ = ready.send(());
        std::future::pending::<()>().await;
        anyhow::Ok(())
    });
    wait.await?;
    assert!(second.try_lock_judge_turn(14, 42).await?.is_none());
    task.abort();
    assert!(task.await.is_err_and(|error| error.is_cancelled()));
    reacquire(&second).await?.release().await?;
    assert_eq!(first.judge_ownership_capacity.turns.available_permits(), 2);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn idle_transaction_timeout_does_not_expire_session_ownership() -> anyhow::Result<()> {
    let Some((db, first, second)) = handles().await? else {
        return Ok(());
    };
    let mut owner = reacquire(&first).await?;
    sqlx::query("SET idle_in_transaction_session_timeout = '100ms'")
        .execute(&mut owner.connection)
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    owner.check_connection().await?;
    assert!(second.try_lock_judge_turn(14, 42).await?.is_none());
    owner.release().await?;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn lost_connection_is_reported_and_other_handle_can_acquire() -> anyhow::Result<()> {
    let Some((db, first, second)) = handles().await? else {
        return Ok(());
    };
    let mut owner = reacquire(&first).await?;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut owner.connection)
        .await?;
    sqlx::query("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .execute(second.pool())
        .await?;
    assert!(owner.check_connection().await.is_err());
    assert!(owner.release().await.is_err());
    reacquire(&second).await?.release().await?;
    assert_eq!(first.judge_ownership_capacity.turns.available_permits(), 2);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn release_reports_missing_lock_and_returns_capacity() -> anyhow::Result<()> {
    let Some((db, first, second)) = handles().await? else {
        return Ok(());
    };
    let mut owner = reacquire(&first).await?;
    sqlx::query("SELECT pg_advisory_unlock_all()")
        .execute(&mut owner.connection)
        .await?;
    assert!(matches!(
        owner.release().await,
        Err(PersistError::DataInvariant(_))
    ));
    reacquire(&second).await?.release().await?;
    assert_eq!(first.judge_ownership_capacity.turns.available_permits(), 2);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn failed_connect_returns_turn_and_history_capacity() -> anyhow::Result<()> {
    let Some((db, first, _second)) = handles().await? else {
        return Ok(());
    };
    first.pool.set_connect_options(
        "postgres://proxy_test:proxy_test@127.0.0.1:1/claude_proxy_never".parse()?,
    );
    assert!(first.try_lock_judge_turn(14, 42).await.is_err());
    assert!(first.try_lock_judge_history(42).await.is_err());
    assert_eq!(first.judge_ownership_capacity.turns.available_permits(), 2);
    assert_eq!(
        first.judge_ownership_capacity.history.available_permits(),
        1
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn connection_deadline_reports_stalled_io() {
    let stalled = std::future::pending::<Result<(), sqlx::Error>>();
    let result = connection_deadline(stalled).await;
    assert!(matches!(
        result,
        Err(PersistError::Database(sqlx::Error::Io(error)))
            if error.kind() == io::ErrorKind::TimedOut
    ));
}
