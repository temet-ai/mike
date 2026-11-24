use serde_json::json;
use uuid::Uuid;

use crate::runtime::codex_proxy_identity::{ProxySource, resolve};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use crate::store::persistence::test_support::TestDb;

#[tokio::test]
async fn db_notification_target_proxy_binding_requires_matching_home_thread_and_open_owner()
-> anyhow::Result<()> {
    let test_db = TestDb::bootstrap()
        .await?
        .ok_or_else(|| anyhow::anyhow!("notification target tests need the disposable database"))?;
    let db = test_db.acquire_isolated().await?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let home = tempfile::tempdir()?;
    let home = home.path().canonicalize()?;
    let home_str = home.to_string_lossy();
    let launch = Uuid::new_v4();
    sqlx::query!("INSERT INTO launch.launches(session_uuid, backend, folder, identity) VALUES($1,'tmux','/repo',$2)",
        launch, json!({"runtime":"codex","runtime_home":home}))
        .execute(persist.pool()).await?;
    let mut source = ProxySource {
        launch: Some(launch),
        home: None,
        thread: Uuid::new_v4().to_string(),
        cwd: None,
    };
    let root = resolve(&persist, &source).await?;
    persist
        .runtime()
        .register_runtime_notification_target(root.session_id, &source.thread, "/wrong-home")
        .await?;
    assert!(
        persist
            .runtime()
            .runtime_notification_target(root.session_id)
            .await?
            .is_none()
    );
    persist
        .runtime()
        .register_runtime_notification_target(root.session_id, "wrong-thread", &home_str)
        .await?;
    assert!(
        persist
            .runtime()
            .runtime_notification_target(root.session_id)
            .await?
            .is_none()
    );
    persist
        .runtime()
        .register_runtime_notification_target(root.session_id, &source.thread, &home_str)
        .await?;
    assert_eq!(
        persist
            .runtime()
            .runtime_notification_target(root.session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("root target missing"))?
            .thread_id,
        source.thread
    );
    source.thread = Uuid::new_v4().to_string();
    let child = resolve(&persist, &source).await?;
    persist
        .runtime()
        .register_runtime_notification_target(child.session_id, &source.thread, &home_str)
        .await?;
    assert!(
        persist
            .runtime()
            .runtime_notification_target(child.session_id)
            .await?
            .is_some()
    );
    assert_unmanaged_target(&persist, &home).await?;
    let facts = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM runtime.session_facts"#)
        .fetch_one(persist.pool())
        .await?;
    assert_eq!(facts, 0);
    sqlx::query!(
        "UPDATE launch.launches SET closed_at = now() WHERE session_uuid = $1",
        launch
    )
    .execute(persist.pool())
    .await?;
    assert_targets_missing(&persist, &[root.session_id, child.session_id]).await?;
    assert!(
        persist
            .runtime()
            .register_runtime_notification_target(root.session_id, "", &home_str)
            .await
            .is_err()
    );
    persist.pool().close().await;
    db.cleanup().await
}

async fn assert_targets_missing(persist: &PersistHandle, sessions: &[Uuid]) -> anyhow::Result<()> {
    for session in sessions {
        assert!(
            persist
                .runtime()
                .runtime_notification_target(*session)
                .await?
                .is_none()
        );
    }
    Ok(())
}

async fn assert_unmanaged_target(
    persist: &PersistHandle,
    home: &std::path::Path,
) -> anyhow::Result<()> {
    let source = ProxySource {
        launch: None,
        home: Some(home.to_path_buf()),
        thread: Uuid::new_v4().to_string(),
        cwd: None,
    };
    let identity = resolve(persist, &source).await?;
    let home = home.to_string_lossy();
    for (thread, candidate_home) in [
        ("wrong-thread", home.as_ref()),
        (source.thread.as_str(), "/wrong-home"),
    ] {
        persist
            .runtime()
            .register_runtime_notification_target(identity.session_id, thread, candidate_home)
            .await?;
        assert_targets_missing(persist, &[identity.session_id]).await?;
    }
    persist
        .runtime()
        .register_runtime_notification_target(identity.session_id, &source.thread, &home)
        .await?;
    assert_eq!(
        persist
            .runtime()
            .runtime_notification_target(identity.session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("unmanaged target missing"))?
            .thread_id,
        source.thread
    );
    Ok(())
}
