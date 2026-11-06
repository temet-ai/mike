use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch::LinkOutcome;

use crate::test_db::TestDb;

#[tokio::test]
async fn link_channel_to_session_linked_once_then_already_linked_or_no_channel()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let session = Uuid::new_v4();
    let other = Uuid::new_v4();
    let cse_absent = format!("cse_absent_{}", Uuid::new_v4());
    let cse_fresh = format!("cse_fresh_{}", Uuid::new_v4());

    assert_eq!(
        handle
            .link_channel_to_session(&cse_absent, session, "/tmp/p")
            .await?,
        LinkOutcome::NoChannel
    );
    assert!(!handle.session_has_remote_control_channel(session).await?);

    sqlx::query("INSERT INTO remote_control.channels (cse_id) VALUES ($1)")
        .bind(&cse_fresh)
        .execute(handle.pool())
        .await?;
    assert_eq!(
        handle
            .link_channel_to_session(&cse_fresh, session, "/tmp/p")
            .await?,
        LinkOutcome::Linked
    );
    assert!(handle.session_has_remote_control_channel(session).await?);

    assert_eq!(
        handle
            .link_channel_to_session(&cse_fresh, other, "/tmp/other")
            .await?,
        LinkOutcome::AlreadyLinked
    );
    assert!(handle.session_has_remote_control_channel(session).await?);
    assert!(!handle.session_has_remote_control_channel(other).await?);
    let (linked, cwd): (Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT local_session_uuid, local_cwd FROM remote_control.channels WHERE cse_id = $1",
    )
    .bind(&cse_fresh)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(linked, Some(session));
    assert_eq!(cwd.as_deref(), Some("/tmp/p"));

    Ok(())
}

#[tokio::test]
async fn link_channel_to_session_existing_cwd_is_kept() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let session = Uuid::new_v4();
    let cse_cwd = format!("cse_cwd_{}", Uuid::new_v4());
    sqlx::query("INSERT INTO remote_control.channels (cse_id, local_cwd) VALUES ($1, '/recorded')")
        .bind(&cse_cwd)
        .execute(handle.pool())
        .await?;
    assert_eq!(
        handle
            .link_channel_to_session(&cse_cwd, session, "/from-launcher")
            .await?,
        LinkOutcome::Linked
    );
    let cwd: Option<String> =
        sqlx::query_scalar("SELECT local_cwd FROM remote_control.channels WHERE cse_id = $1")
            .bind(&cse_cwd)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(cwd.as_deref(), Some("/recorded"));
    Ok(())
}
