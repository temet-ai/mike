use serde_json::json;

use super::*;
use crate::store::persistence::test_support;

#[tokio::test]
async fn wire_binds_a_spawn_and_children_without_hooks() -> anyhow::Result<()> {
    let db = test_support::acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("proxy identity check requires the test database"))?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let home = tempfile::tempdir()?;
    let home = home.path().canonicalize()?;
    let launch = Uuid::new_v4();
    sqlx::query("INSERT INTO launch.launches(session_uuid, backend, folder, identity) VALUES($1,'tmux','/repo',$2)")
        .bind(launch).bind(json!({"runtime":"codex","runtime_home":home}))
        .execute(persist.pool()).await?;
    let mut source = ProxySource {
        launch: Some(launch),
        home: None,
        thread: Uuid::new_v4().to_string(),
        cwd: None,
    };
    let root = resolve(&persist, &source).await?;
    assert_eq!(root.session_id, launch);
    assert_eq!(root.cwd.as_deref(), Some("/repo"));
    assert_eq!(root.codex_home.as_deref(), Some(home.as_path()));
    assert_eq!(resolve(&persist, &source).await?.session_id, launch);
    let parent_thread = source.thread.clone();
    source.thread = Uuid::new_v4().to_string();
    let child = resolve(&persist, &source).await?;
    assert_ne!(child.session_id, launch);
    let binding = persist
        .runtime()
        .runtime_thread_by_session(child.session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing child binding"))?;
    assert_eq!(binding.owning_seat_id, Some(launch));
    assert_eq!(
        binding.native_parent.map(|provider| provider.thread_id),
        Some(parent_thread)
    );
    source.launch = None;
    assert_eq!(
        resolve(&persist, &source).await?.session_id,
        child.session_id
    );
    source.launch = Some(Uuid::new_v4());
    assert!(resolve(&persist, &source).await.is_err());
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn ordinary_wire_identity_is_stable_and_unknown_launch_cannot_adopt_it() -> anyhow::Result<()>
{
    let db = test_support::acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("proxy identity check requires the test database"))?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let mut source = ProxySource {
        launch: None,
        home: None,
        thread: Uuid::new_v4().to_string(),
        cwd: Some("/repo".into()),
    };
    let first = resolve(&persist, &source).await?;
    assert_eq!(
        resolve(&persist, &source).await?.session_id,
        first.session_id
    );
    source.launch = Some(Uuid::new_v4());
    assert!(resolve(&persist, &source).await.is_err());
    source.launch = None;
    source.thread.clear();
    assert!(resolve(&persist, &source).await.is_err());
    persist.pool().close().await;
    db.cleanup().await
}
