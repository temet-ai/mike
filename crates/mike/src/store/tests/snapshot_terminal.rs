use super::*;

async fn test_persist_handle() -> anyhow::Result<
    Option<(
        crate::store::persistence::PersistHandle,
        crate::store::persistence::test_support::TestDbHandle,
    )>,
> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = crate::store::persistence::PersistHandle::connect(&db.database_url).await?;
    Ok(Some((handle, db)))
}

async fn direct_pool_for(database_url: &str) -> anyhow::Result<sqlx::PgPool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(database_url)
        .await?;
    Ok(pool)
}

fn insert_terminal_request_for_snapshot(store: &mut RequestStore, session_id: &str) -> RequestId {
    let mut req = make_test_request(None, session_id);
    req.status = RequestStatus::Complete;
    req.request_body = Some(bytes::Bytes::from_static(b"hello-request"));
    req.response_body = Some(bytes::Bytes::from_static(b"hello-response"));
    req.completed_at = Some(Utc::now());
    let id = req.id;
    store.insert(req);
    id
}

use crate::store::persistence::test_support;

#[tokio::test]
async fn snapshot_terminal_persists_unpinned_request() -> anyhow::Result<()> {
    let Some((handle, db)) = test_persist_handle().await? else {
        return Ok(());
    };
    let pool = direct_pool_for(&db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();

    let mut store = RequestStore::new(small_policy(10));
    store.set_persist(handle.clone());
    let id = insert_terminal_request_for_snapshot(&mut store, &session_id);
    let store = std::sync::Arc::new(tokio::sync::RwLock::new(store));

    RequestStore::snapshot_terminal(&store, id).await;

    let count = test_support::request_row_count(&pool, &session_id).await?;
    assert_eq!(count, 1, "expected one persisted row for this session");

    let session_db_id = handle
        .resolve_session_db_id(&session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session must exist after snapshot"))?;
    let eff = handle.effective_pinned_for_session(session_db_id).await?;
    assert!(
        !eff,
        "session+project both unpinned → effective_pinned=false"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn snapshot_terminal_persists_pinned_via_session() -> anyhow::Result<()> {
    let Some((handle, db)) = test_persist_handle().await? else {
        return Ok(());
    };
    let pool = direct_pool_for(&db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();

    let mut store = RequestStore::new(small_policy(10));
    store.set_persist(handle.clone());
    let id1 = insert_terminal_request_for_snapshot(&mut store, &session_id);
    let store = std::sync::Arc::new(tokio::sync::RwLock::new(store));

    RequestStore::snapshot_terminal(&store, id1).await;
    test_support::pin_session_row(&pool, &session_id).await?;

    let session_db_id = handle
        .resolve_session_db_id(&session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session must exist after snapshot"))?;
    let eff = handle.effective_pinned_for_session(session_db_id).await?;
    assert!(
        eff,
        "session.pinned=true must surface as effective_pinned=true"
    );

    let id2 = {
        let mut s = store.write().await;
        let mut req = make_test_request(None, &session_id);
        req.status = RequestStatus::Complete;
        req.request_body = Some(bytes::Bytes::from_static(b"second-request"));
        req.response_body = Some(bytes::Bytes::from_static(b"second-response"));
        req.created_at = Utc::now() + ChronoDuration::seconds(1);
        req.completed_at = Some(req.created_at);
        let id = req.id;
        s.insert(req);
        id
    };

    RequestStore::snapshot_terminal(&store, id2).await;

    let count = test_support::request_row_count(&pool, &session_id).await?;
    assert_eq!(count, 2, "two snapshot_terminal calls → two rows");

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn snapshot_terminal_persists_pinned_via_project_cascade() -> anyhow::Result<()> {
    let Some((handle, db)) = test_persist_handle().await? else {
        return Ok(());
    };
    let pool = direct_pool_for(&db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();

    let mut store = RequestStore::new(small_policy(10));
    store.set_persist(handle.clone());
    let id = insert_terminal_request_for_snapshot(&mut store, &session_id);
    let store = std::sync::Arc::new(tokio::sync::RwLock::new(store));

    RequestStore::snapshot_terminal(&store, id).await;
    test_support::pin_project_for_session(&pool, &session_id).await?;

    let session_pinned = test_support::session_pinned(&pool, &session_id).await?;
    assert!(!session_pinned, "session pin must still be false");

    let session_db_id = handle
        .resolve_session_db_id(&session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session must exist after snapshot"))?;
    let eff = handle.effective_pinned_for_session(session_db_id).await?;
    assert!(
        eff,
        "project.pinned=true with session.pinned=false must cascade to effective_pinned=true"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
