use uuid::Uuid;

use super::RuntimeWriteOutcome;
use super::tests::{binding, database, namespace};
use crate::runtime::{ProviderThreadRef, RuntimeKind};
use crate::store::persistence::{PersistError, test_support};

#[tokio::test]
async fn namespace_replay_and_conflicting_durable_scope_are_explicit() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    assert_eq!(
        handle.register_runtime_namespace(&namespace).await?,
        RuntimeWriteOutcome::Unchanged
    );
    let mut changed = namespace.clone();
    changed.id = Uuid::new_v4();
    assert!(matches!(
        handle.register_runtime_namespace(&changed).await,
        Err(PersistError::DataInvariant(_))
    ));
    changed = namespace.clone();
    changed.runtime = RuntimeKind::Claude;
    assert!(matches!(
        handle.register_runtime_namespace(&changed).await,
        Err(PersistError::DataInvariant(_))
    ));
    assert_eq!(
        handle.runtime_namespace(namespace.id).await?,
        Some(namespace)
    );
    assert!(handle.runtime_namespace(Uuid::new_v4()).await?.is_none());
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn shared_provider_session_has_distinct_child_and_unresolved_fork() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let project = test_support::seed_project(handle.pool(), "runtime-parent-child-fixture").await?;
    let seat = Uuid::new_v4();
    let child = Uuid::new_v4();
    test_support::seed_two_sessions(
        handle.pool(),
        &seat.to_string(),
        &child.to_string(),
        project,
    )
    .await?;
    let parent = binding(namespace.id, seat);
    let mut descendant = binding(namespace.id, child);
    descendant.provider.thread_id = "child:NonUUID/Case".into();
    descendant.owning_seat_id = Some(seat);
    descendant.native_parent = Some(parent.provider.clone());
    descendant.forked_from = Some(ProviderThreadRef {
        namespace_id: namespace.id,
        thread_id: "fork:unresolved".into(),
    });
    assert_eq!(
        handle.bind_runtime_thread(&descendant).await?,
        RuntimeWriteOutcome::Inserted
    );
    assert!(handle.runtime_thread(&parent.provider).await?.is_none());
    assert_eq!(
        handle.bind_runtime_thread(&parent).await?,
        RuntimeWriteOutcome::Inserted
    );
    assert_eq!(
        handle.bind_runtime_thread(&descendant).await?,
        RuntimeWriteOutcome::Unchanged
    );
    assert_eq!(
        handle.runtime_thread_by_session(child).await?,
        Some(descendant.clone())
    );
    assert_eq!(handle.runtime_thread(&parent.provider).await?, Some(parent));
    let mut changed = descendant.clone();
    changed.forked_from = None;
    assert!(matches!(
        handle.bind_runtime_thread(&changed).await,
        Err(PersistError::DataInvariant(_))
    ));
    changed = descendant;
    changed.mike_session_id = seat;
    changed.native_parent = None;
    assert!(matches!(
        handle.bind_runtime_thread(&changed).await,
        Err(PersistError::DataInvariant(_))
    ));
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn provider_thread_id_is_scoped_by_durable_namespace() -> anyhow::Result<()> {
    let Some((db, handle, first_namespace)) = database().await? else {
        return Ok(());
    };
    let second_namespace = namespace();
    handle.register_runtime_namespace(&second_namespace).await?;
    let project =
        test_support::seed_project(handle.pool(), "runtime-scoped-identity-fixture").await?;
    let first_session = Uuid::new_v4();
    let second_session = Uuid::new_v4();
    test_support::seed_two_sessions(
        handle.pool(),
        &first_session.to_string(),
        &second_session.to_string(),
        project,
    )
    .await?;
    let first = binding(first_namespace.id, first_session);
    let second = binding(second_namespace.id, second_session);
    assert_eq!(
        handle.bind_runtime_thread(&first).await?,
        RuntimeWriteOutcome::Inserted
    );
    assert_eq!(
        handle.bind_runtime_thread(&second).await?,
        RuntimeWriteOutcome::Inserted
    );
    assert_eq!(handle.runtime_thread(&first.provider).await?, Some(first));
    assert_eq!(handle.runtime_thread(&second.provider).await?, Some(second));
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn thread_binding_missing_session_or_owner_fails_without_writes() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let mut identity = binding(namespace.id, Uuid::new_v4());
    assert!(matches!(
        handle.bind_runtime_thread(&identity).await,
        Err(PersistError::DataInvariant(_))
    ));
    let project =
        test_support::seed_project(handle.pool(), "runtime-missing-owner-fixture").await?;
    test_support::seed_two_sessions(
        handle.pool(),
        &identity.mike_session_id.to_string(),
        &Uuid::new_v4().to_string(),
        project,
    )
    .await?;
    identity.owning_seat_id = Some(Uuid::new_v4());
    assert!(matches!(
        handle.bind_runtime_thread(&identity).await,
        Err(PersistError::DataInvariant(_))
    ));
    assert!(handle.runtime_thread(&identity.provider).await?.is_none());
    assert!(
        handle
            .runtime_thread_by_session(identity.mike_session_id)
            .await?
            .is_none()
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn deleted_owner_clears_live_fk_and_preserves_historical_binding() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let project =
        test_support::seed_project(handle.pool(), "runtime-deleted-owner-fixture").await?;
    let seat = Uuid::new_v4();
    let child = Uuid::new_v4();
    test_support::seed_two_sessions(
        handle.pool(),
        &seat.to_string(),
        &child.to_string(),
        project,
    )
    .await?;
    let parent = binding(namespace.id, seat);
    let mut descendant = binding(namespace.id, child);
    descendant.provider.thread_id = "child:historical-owner".into();
    descendant.owning_seat_id = Some(seat);
    descendant.native_parent = Some(parent.provider.clone());
    handle.bind_runtime_thread(&parent).await?;
    handle.bind_runtime_thread(&descendant).await?;
    sqlx::query!(
        "DELETE FROM sessions.sessions WHERE session_uuid = $1",
        seat
    )
    .execute(handle.pool())
    .await?;
    let resolved_owner = sqlx::query_scalar!(
        "SELECT owning_seat_id FROM runtime.threads WHERE namespace_id = $1 AND provider_thread_id = $2",
        namespace.id,
        descendant.provider.thread_id,
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(resolved_owner, None);
    assert_eq!(
        handle.runtime_thread_by_session(child).await?,
        Some(descendant.clone())
    );
    assert!(handle.runtime_thread(&parent.provider).await?.is_none());
    assert_eq!(
        handle.bind_runtime_thread(&descendant).await?,
        RuntimeWriteOutcome::Unchanged
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
