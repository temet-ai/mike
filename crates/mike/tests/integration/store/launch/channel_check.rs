use uuid::Uuid;

use mike::store::persistence::PersistHandle;

use crate::test_db::TestDb;

#[tokio::test]
async fn channel_check_false_until_a_channel_links_the_session() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let session = Uuid::new_v4();
    let other = Uuid::new_v4();
    let cse_other = format!("cse_other_{}", Uuid::new_v4());
    let cse_unlinked = format!("cse_unlinked_{}", Uuid::new_v4());
    let cse_mine = format!("cse_mine_{}", Uuid::new_v4());

    assert!(!handle.session_has_remote_control_channel(session).await?);

    sqlx::query("INSERT INTO remote_control.channels (cse_id, local_session_uuid) VALUES ($1, $3), ($2, NULL)")
        .bind(&cse_other)
        .bind(&cse_unlinked)
        .bind(other)
        .execute(handle.pool())
        .await?;
    assert!(!handle.session_has_remote_control_channel(session).await?);

    sqlx::query("INSERT INTO remote_control.channels (cse_id, local_session_uuid) VALUES ($1, $2)")
        .bind(&cse_mine)
        .bind(session)
        .execute(handle.pool())
        .await?;
    assert!(handle.session_has_remote_control_channel(session).await?);

    Ok(())
}
