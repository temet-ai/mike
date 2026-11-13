use crate::store::persistence::web_repo::WebPersist as _;

use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::id_resolvers::resolve_session_id;
use crate::store::persistence::test_support::{TestDb, seed_project};
use crate::store::persistence::workspace::{PathMatch, ProjectPathResolution};

async fn shared_handle() -> anyhow::Result<Option<(PersistHandle, PersistHandle)>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire().await?;
    let cached = PersistHandle::connect(&db.database_url).await?;
    let fresh = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((cached, fresh)))
}

async fn seed_session(handle: &PersistHandle) -> anyhow::Result<(String, i64)> {
    let path = format!("/id-cache/{}", Uuid::new_v4());
    let project = seed_project(handle.pool(), &path).await?;
    let session = Uuid::new_v4().to_string();
    let id = resolve_session_id(handle.pool(), &session, Some(project), None).await?;
    Ok((session, id))
}

async fn delete_session(handle: &PersistHandle, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions.sessions WHERE id = $1")
        .bind(id)
        .execute(handle.pool())
        .await?;
    Ok(())
}

#[tokio::test]
async fn session_lookup_cached_hit_answers_without_the_row() -> anyhow::Result<()> {
    let Some((cached, fresh)) = shared_handle().await? else {
        return Ok(());
    };
    let (session, id) = seed_session(&cached).await?;
    assert_eq!(cached.resolve_session_db_id(&session).await?, Some(id));
    delete_session(&cached, id).await?;
    assert_eq!(cached.resolve_session_db_id(&session).await?, Some(id));
    assert_eq!(fresh.resolve_session_db_id(&session).await?, None);
    Ok(())
}

#[tokio::test]
async fn session_lookup_miss_falls_through_to_the_row() -> anyhow::Result<()> {
    let Some((cached, _)) = shared_handle().await? else {
        return Ok(());
    };
    let unknown = Uuid::new_v4();
    assert_eq!(cached.session_row_id(unknown).await?, None);
    let (session, id) = seed_session(&cached).await?;
    assert_eq!(cached.resolve_session_db_id(&session).await?, Some(id));
    Ok(())
}

#[tokio::test]
async fn session_lookup_not_a_uuid_returns_none() -> anyhow::Result<()> {
    let Some((cached, _)) = shared_handle().await? else {
        return Ok(());
    };
    assert_eq!(cached.resolve_session_db_id("not-a-uuid").await?, None);
    Ok(())
}

#[tokio::test]
async fn session_sweep_removing_rows_forgets_cached_ids() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (session, id) = seed_session(&handle).await?;
    assert_eq!(handle.resolve_session_db_id(&session).await?, Some(id));
    let far_future = chrono::Utc::now() + chrono::Duration::days(3650);
    let removed = handle
        .sweep_expired_sessions(std::time::Duration::from_secs(1), far_future)
        .await?;
    assert!(removed >= 1, "the sweep must remove the seeded session");
    assert_eq!(handle.resolve_session_db_id(&session).await?, None);
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

async fn rename_project(handle: &PersistHandle, id: i64, path: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE projects.projects SET project_path = $2 WHERE id = $1")
        .bind(id)
        .bind(path)
        .execute(handle.pool())
        .await?;
    Ok(())
}

#[tokio::test]
async fn project_lookup_cached_hit_survives_a_renamed_row() -> anyhow::Result<()> {
    let Some((cached, fresh)) = shared_handle().await? else {
        return Ok(());
    };
    let path = format!("/id-cache/{}", Uuid::new_v4());
    let id = seed_project(cached.pool(), &path).await?;
    assert_eq!(cached.find_project_id_by_path(&path).await?, Some(id));
    rename_project(&cached, id, &format!("{path}-renamed")).await?;
    assert_eq!(cached.find_project_id_by_path(&path).await?, Some(id));
    assert_eq!(fresh.find_project_id_by_path(&path).await?, None);
    Ok(())
}

#[tokio::test]
async fn project_merge_forgets_cached_paths() -> anyhow::Result<()> {
    let Some((cached, _)) = shared_handle().await? else {
        return Ok(());
    };
    let base = format!("/id-cache/{}", Uuid::new_v4());
    let watched = format!("{base}/watched");
    let old_id = seed_project(cached.pool(), &watched).await?;
    assert_eq!(
        cached.find_project_id_by_path(&watched).await?,
        Some(old_id)
    );
    rename_project(&cached, old_id, &format!("{watched}-old")).await?;
    let new_id = seed_project(cached.pool(), &watched).await?;
    let source = format!("{base}/source");
    seed_project(cached.pool(), &source).await?;
    let target = seed_project(cached.pool(), &format!("{base}/target")).await?;
    let target_uuid: Uuid =
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(target)
            .fetch_one(cached.pool())
            .await?;
    cached
        .web()
        .merge_project_into(&source, target_uuid)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    assert_eq!(
        cached.find_project_id_by_path(&watched).await?,
        Some(new_id)
    );
    Ok(())
}

#[tokio::test]
async fn project_suffix_match_is_not_an_exact_answer() -> anyhow::Result<()> {
    let Some((cached, _)) = shared_handle().await? else {
        return Ok(());
    };
    let leaf = format!("leaf-{}", Uuid::new_v4());
    let path = format!("/id-cache/mid/{leaf}");
    let id = seed_project(cached.pool(), &path).await?;
    let suffix = format!("mid/{leaf}");
    let resolution = cached.resolve_project(&suffix, PathMatch::OrSuffix).await?;
    let ProjectPathResolution::Resolved(found, canonical) = resolution else {
        anyhow::bail!("suffix {suffix} must resolve to {path}, got {resolution:?}");
    };
    assert_eq!((found, canonical.as_str()), (id, path.as_str()));
    assert_eq!(cached.find_project_id_by_path(&suffix).await?, None);
    Ok(())
}
