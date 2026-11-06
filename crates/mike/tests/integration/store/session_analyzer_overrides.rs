use mike::store::persistence::{PersistError, PersistHandle};
use mike::store::test_support::TestDb;

async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(handle))
}

async fn seed_session(pool: &sqlx::PgPool, label: &str) -> anyhow::Result<uuid::Uuid> {
    let unique_label = format!("{label}-{}", uuid::Uuid::new_v4().simple());
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        unique_label,
    )
    .fetch_one(pool)
    .await?;

    let session_uuid = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())",
        session_uuid,
        project_id,
    )
    .execute(pool)
    .await?;
    Ok(session_uuid)
}

#[tokio::test]
async fn set_override_unknown_session_returns_data_invariant() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let result = handle
        .set_session_analyzer_override(uuid::Uuid::new_v4(), "overselling_analyzer", false)
        .await;
    assert!(
        matches!(result, Err(PersistError::DataInvariant(_))),
        "expected DataInvariant, got {result:?}"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn set_override_then_get_round_trips() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_uuid = seed_session(handle.pool(), "proj-analyzer-override-round-trip").await?;

    handle
        .set_session_analyzer_override(session_uuid, "overselling_analyzer", false)
        .await?;
    handle
        .set_session_analyzer_override(session_uuid, "weasel_intercept", true)
        .await?;

    let overrides = handle.get_session_analyzer_overrides(session_uuid).await?;
    assert_eq!(overrides.get("overselling_analyzer"), Some(&false));
    assert_eq!(overrides.get("weasel_intercept"), Some(&true));
    assert_eq!(overrides.len(), 2);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn set_override_twice_upserts_not_duplicates() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_uuid = seed_session(handle.pool(), "proj-analyzer-override-upsert").await?;

    handle
        .set_session_analyzer_override(session_uuid, "overselling_analyzer", false)
        .await?;
    handle
        .set_session_analyzer_override(session_uuid, "overselling_analyzer", true)
        .await?;

    let overrides = handle.get_session_analyzer_overrides(session_uuid).await?;
    assert_eq!(overrides.get("overselling_analyzer"), Some(&true));
    assert_eq!(overrides.len(), 1, "upsert must not create a duplicate row");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn get_overrides_for_session_with_none_set_returns_empty() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_uuid = seed_session(handle.pool(), "proj-analyzer-override-empty").await?;

    let overrides = handle.get_session_analyzer_overrides(session_uuid).await?;
    assert!(overrides.is_empty());

    handle.pool().close().await;
    Ok(())
}
