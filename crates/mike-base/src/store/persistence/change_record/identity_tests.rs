use uuid::Uuid;

use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{
    acquire_ephemeral_db, seed_project, seed_two_sessions,
};

fn fed(handle: &PersistHandle) -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    crate::change_feed::spawn_change_feed(
        &feed,
        Some(handle),
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

async fn name(handle: &PersistHandle, uuid: Uuid, name: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO sessions.session_goals (session_id, goal, display_name)
         SELECT id, 'g', $2 FROM sessions.sessions WHERE session_uuid = $1
         ON CONFLICT (session_id) DO UPDATE SET display_name = EXCLUDED.display_name",
    )
    .bind(uuid)
    .bind(name)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_kept_identity_holds_until_its_session_changes() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/test/identity").await?;
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    seed_two_sessions(handle.pool(), &a.to_string(), &b.to_string(), project).await?;
    name(&handle, a, "first").await?;
    let feed = fed(&handle);

    let read = handle.session_identity_by_uuid(a).await?;
    let Some(read) = read else {
        anyhow::bail!("the session exists");
    };
    assert_eq!(read.name.as_deref(), Some("first"));

    name(&handle, a, "second").await?;
    let kept = handle.session_identities(&[read.id]).await?;
    assert_eq!(
        kept[0].name.as_deref(),
        Some("first"),
        "kept until a notice"
    );

    feed.record(RowChangedTable::Sessions, Some(&b.to_string()));
    let other = handle.session_identity_by_uuid(a).await?;
    assert_eq!(
        other.and_then(|i| i.name).as_deref(),
        Some("first"),
        "another session's notice"
    );

    feed.record(RowChangedTable::Sessions, Some(&a.to_string()));
    let fresh = handle.session_identities(&[read.id]).await?;
    assert_eq!(fresh[0].name.as_deref(), Some("second"));
    db.cleanup().await
}

#[tokio::test]
async fn a_display_name_change_sends_the_sessions_notice() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/test/identity-notice").await?;
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    seed_two_sessions(handle.pool(), &a.to_string(), &b.to_string(), project).await?;
    let mut listener = sqlx::postgres::PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    name(&handle, a, "named").await?;
    let note = tokio::time::timeout(std::time::Duration::from_secs(5), listener.recv()).await??;
    let payload: serde_json::Value = serde_json::from_str(note.payload())?;
    assert_eq!(payload["table"], "sessions");
    assert_eq!(payload["session_id"], a.to_string());
    db.cleanup().await
}

async fn next_notice(
    listener: &mut sqlx::postgres::PgListener,
) -> anyhow::Result<serde_json::Value> {
    let note = tokio::time::timeout(std::time::Duration::from_secs(5), listener.recv()).await??;
    Ok(serde_json::from_str(note.payload())?)
}

async fn next_notice_for(
    listener: &mut sqlx::postgres::PgListener,
    table: &str,
) -> anyhow::Result<serde_json::Value> {
    loop {
        let payload = next_notice(listener).await?;
        if payload["table"] == table {
            return Ok(payload);
        }
    }
}

#[tokio::test]
async fn a_deleted_session_sends_its_notice_and_leaves_the_memo() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/test/identity-delete").await?;
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    seed_two_sessions(handle.pool(), &a.to_string(), &b.to_string(), project).await?;
    let feed = fed(&handle);
    assert!(handle.session_identity_by_uuid(a).await?.is_some());

    let mut listener = sqlx::postgres::PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    sqlx::query("DELETE FROM sessions.sessions WHERE session_uuid = $1")
        .bind(a)
        .execute(handle.pool())
        .await?;
    let payload = next_notice_for(&mut listener, "sessions").await?;
    assert_eq!(payload["op"], "DELETE");
    assert_eq!(payload["session_id"], a.to_string());

    feed.record(RowChangedTable::Sessions, Some(&a.to_string()));
    assert_eq!(
        handle.session_identity_by_uuid(a).await?,
        None,
        "the deleted session is gone"
    );
    db.cleanup().await
}
