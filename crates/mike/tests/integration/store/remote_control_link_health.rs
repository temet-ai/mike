use mike::store::persistence::PersistHandle;
use mike::store::persistence::remote_control::ChannelLinkCounts;
use mike::store::test_support::TestDb;

async fn channel(handle: &PersistHandle, minutes_ago: i32, linked: bool) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO remote_control.channels (cse_id, first_seen_at, local_session_uuid)
         VALUES ('cse_' || gen_random_uuid(), now() - make_interval(mins => $1),
                 CASE WHEN $2 THEN gen_random_uuid() END)",
    )
    .bind(minutes_ago)
    .bind(linked)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn recent_channel_link_counts_counts_settled_channels_of_the_last_day() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping remote_control_link_health test");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    channel(&handle, 60, true).await?;
    channel(&handle, 120, true).await?;
    channel(&handle, 180, false).await?;
    channel(&handle, 5, false).await?;
    channel(&handle, 60 * 30, false).await?;

    assert_eq!(
        handle.recent_channel_link_counts().await?,
        ChannelLinkCounts {
            channels: 3,
            linked: 2
        },
        "only channels first seen between 24 hours and 15 minutes ago count"
    );

    handle.pool().close().await;
    assert!(handle.recent_channel_link_counts().await.is_err());
    db.cleanup().await
}
