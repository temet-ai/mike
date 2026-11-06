use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::workspace::RegisterWorkspaceSession;
use mike::store::test_support::TestDb;

pub async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(handle))
}

pub async fn shared_pool_and_url() -> anyhow::Result<Option<(sqlx::PgPool, String)>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    Ok(Some((pool, db.database_url.clone())))
}

pub fn unique(label: &str) -> String {
    format!("/tmp/{label}-{}", Uuid::new_v4())
}

pub fn register_input(session_uuid: Uuid, project_path: &str) -> RegisterWorkspaceSession<'_> {
    RegisterWorkspaceSession {
        session_uuid,
        project_path: Some(project_path),
        cwd: project_path,
        worktree: None,
        registered_via: "hook",
        intent: None,
        role: None,
        name: None,
        instructions: None,
        pid: None,
    }
}
