use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn write(handle: &PersistHandle, value: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ('test.memo', to_jsonb($1::bigint))
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(value)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_setting_is_kept_until_the_config_listener_reloads() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    write(&handle, 1).await?;
    assert_eq!(handle.setting_value("test.memo").await?, Some(1.into()));
    write(&handle, 2).await?;
    assert_eq!(
        handle.setting_value("test.memo").await?,
        Some(2.into()),
        "before the listener first reloads, every read asks the database"
    );

    handle.settings_changed();
    assert_eq!(handle.setting_value("test.memo").await?, Some(2.into()));
    write(&handle, 3).await?;
    assert_eq!(
        handle.setting_value("test.memo").await?,
        Some(2.into()),
        "kept until the reload"
    );
    handle.settings_changed();
    assert_eq!(handle.setting_value("test.memo").await?, Some(3.into()));
    assert_eq!(handle.setting_value("test.absent").await?, None);
    db.cleanup().await
}
