use mike::store::persistence::PersistHandle;
use mike::store::persistence::meta;
use mike::store::retention::RetentionPolicy;
use mike::store::test_support::TestDb;

async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(handle))
}

#[tokio::test]
async fn patch_project_pin_updates_db() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let project_path = format!("patch-proj-{}", uuid::Uuid::new_v4());

    handle.pin_project(&project_path).await?;
    let (pinned,): (bool,) =
        sqlx::query_as("SELECT pinned FROM projects.projects WHERE project_path = $1")
            .bind(&project_path)
            .fetch_one(handle.pool())
            .await?;
    assert!(pinned, "project must be pinned after pin_project");

    handle.unpin_project(&project_path).await?;
    let (pinned,): (bool,) =
        sqlx::query_as("SELECT pinned FROM projects.projects WHERE project_path = $1")
            .bind(&project_path)
            .fetch_one(handle.pool())
            .await?;
    assert!(!pinned, "project must be unpinned after unpin_project");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn effective_pinned_cascades_from_project_when_session_unset() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let session_id = uuid::Uuid::new_v4().to_string();
    let project_path = format!("cascade-proj-{}", uuid::Uuid::new_v4());

    handle.pin_project(&project_path).await?;

    let session_db_id = handle.ensure_session_exists(&session_id).await?;
    let (project_id,): (i64,) =
        sqlx::query_as("SELECT id FROM projects.projects WHERE project_path = $1")
            .bind(&project_path)
            .fetch_one(handle.pool())
            .await?;
    sqlx::query!(
        "UPDATE sessions.sessions SET project_id = $2 WHERE id = $1",
        session_db_id,
        project_id,
    )
    .execute(handle.pool())
    .await?;

    let eff = handle.effective_pinned_for_session(session_db_id).await?;
    assert!(
        eff,
        "cascade: session unpinned + project pinned must resolve to true"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn effective_ttl_falls_back_to_default() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let session_id = uuid::Uuid::new_v4().to_string();
    let default = std::time::Duration::from_secs(3600);

    let ttl = handle.effective_ttl_for_session(0i64, default).await?;
    assert_eq!(ttl, default, "no session row → default TTL");

    let db_id = handle.ensure_session_exists(&session_id).await?;
    let ttl = handle.effective_ttl_for_session(db_id, default).await?;
    assert_eq!(ttl, default, "session with NULL ttl_override → default TTL");

    let twelve_h = std::time::Duration::from_secs(43_200);
    handle.set_session_ttl(&session_id, Some(twelve_h)).await?;
    let ttl = handle.effective_ttl_for_session(db_id, default).await?;
    assert_eq!(ttl, twelve_h, "session ttl_override must beat default");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn patch_session_pin_updates_db() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();

    handle.pin_session(&session_id).await?;
    let (pinned,): (bool,) =
        sqlx::query_as("SELECT pinned FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session_id)
            .fetch_one(handle.pool())
            .await?;
    assert!(pinned, "session must be pinned after pin_session");

    let db_id = handle
        .resolve_session_db_id(&session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session must exist after pin_session"))?;
    handle.unpin_session(db_id).await?;
    let (pinned,): (bool,) =
        sqlx::query_as("SELECT pinned FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session_id)
            .fetch_one(handle.pool())
            .await?;
    assert!(!pinned, "session must be unpinned after unpin_session");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn patch_session_ttl_updates_db() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();

    handle
        .set_session_ttl(&session_id, Some(std::time::Duration::from_secs(7200)))
        .await?;

    let row: (Option<sqlx::postgres::types::PgInterval>,) = sqlx::query_as(
        "SELECT ttl_override FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
    )
    .bind(&session_id)
    .fetch_one(handle.pool())
    .await?;
    let ttl_str = row
        .0
        .and_then(meta::pg_interval_to_humantime)
        .ok_or_else(|| anyhow::anyhow!("expected TTL override to be set"))?;
    let parsed = RetentionPolicy::parse_duration(&ttl_str);
    assert_eq!(parsed, std::time::Duration::from_secs(7200));

    handle.set_session_ttl(&session_id, None).await?;
    let (row,): (Option<sqlx::postgres::types::PgInterval>,) = sqlx::query_as(
        "SELECT ttl_override FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
    )
    .bind(&session_id)
    .fetch_one(handle.pool())
    .await?;
    assert!(row.is_none(), "TTL must be NULL after clearing");

    handle.pool().close().await;
    Ok(())
}
