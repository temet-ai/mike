use mike::store::persistence::PersistHandle;
use mike::store::test_support::TestDb;

async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(handle))
}

fn unique_key(label: &str) -> String {
    format!("test.{label}.{}", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn fetch_settings_empty_table_returns_empty_vec() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let settings = handle.fetch_settings().await?;
    assert!(
        settings.is_empty(),
        "fresh DB must have no settings rows (no seed rows — absent key = compiled default) — \
         this test needs the whole config.settings table empty, so it keeps acquire_isolated"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn upsert_setting_inserts_and_is_fetchable() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let key = unique_key("max_lessons_to_inject");

    handle.upsert_setting(&key, &serde_json::json!(10)).await?;

    let settings = handle.fetch_settings().await?;
    let val = settings
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.clone())
        .ok_or_else(|| anyhow::anyhow!("row must exist after upsert"))?;
    assert_eq!(val, serde_json::json!(10));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn upsert_setting_overwrites_on_conflict() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let key = unique_key("overwrite_target");

    handle.upsert_setting(&key, &serde_json::json!(10)).await?;
    handle.upsert_setting(&key, &serde_json::json!(20)).await?;

    let settings = handle.fetch_settings().await?;
    let val = settings
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.clone())
        .ok_or_else(|| anyhow::anyhow!("row must exist after second upsert"))?;
    assert_eq!(val, serde_json::json!(20), "second upsert must overwrite");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn upsert_setting_bool_value_round_trips() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let key = unique_key("inject_lessons");

    handle
        .upsert_setting(&key, &serde_json::json!(false))
        .await?;

    let settings = handle.fetch_settings().await?;
    let val = settings
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.clone())
        .ok_or_else(|| anyhow::anyhow!("row must exist after upsert"))?;
    assert_eq!(val, serde_json::json!(false));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn upsert_setting_string_list_round_trips() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let key = unique_key("forbidden_words");

    let words = serde_json::json!(["actually", "basically", "simply"]);
    handle.upsert_setting(&key, &words).await?;

    let settings = handle.fetch_settings().await?;
    let val = settings
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.clone())
        .ok_or_else(|| anyhow::anyhow!("row must exist after upsert"))?;
    assert_eq!(val, words);

    handle.pool().close().await;
    Ok(())
}
