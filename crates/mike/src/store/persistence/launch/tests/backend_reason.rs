use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewLaunch;
use crate::store::persistence::test_support;

#[tokio::test]
async fn insert_launch_herdr_core_keeps_its_backend_and_reason() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = Uuid::new_v4();
    let mut new = NewLaunch::new("herdr-core", "/tmp/backend-reason");
    new.session_uuid = Some(session);
    new.backend_reason = Some("the caller runs in Herdr app terminal t13".to_string());
    handle.insert_launch(&new).await?;

    let row = handle
        .find_launch_by_session(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the launch was not found"))?;
    assert_eq!(row.backend, "herdr-core");
    assert_eq!(
        row.backend_reason.as_deref(),
        Some("the caller runs in Herdr app terminal t13")
    );
    db.cleanup().await
}

#[tokio::test]
async fn insert_launch_unknown_backend_is_refused() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let refused = handle
        .insert_launch(&NewLaunch::new("screen", "/tmp/backend-reason"))
        .await;
    assert!(refused.is_err(), "an unknown backend must fail the CHECK");
    db.cleanup().await
}
