use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;

use crate::test_db::TestDb;

use mike::config::SettingsWriteGuard;
use mike::features_settings::RemindersOverlay;
use mike::settings::SettingsOverlay;
use mike::settings::service::{SettingsService, SettingsServiceError};
use mike::store::persistence::PersistHandle;

#[tokio::test]
async fn reconcile_seeds_overlay_from_empty_db() -> anyhow::Result<()> {
    mike::feature_settings::install();
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
    let write_lock = Arc::new(Mutex::new(SettingsWriteGuard));
    let _svc = SettingsService::reconcile_on_startup(overlay.clone(), write_lock, handle).await?;

    let current = overlay.load().features.get::<RemindersOverlay>();
    assert!(
        !current.inject_lessons,
        "default must be false (toggle-off-by-default contract)"
    );
    assert_eq!(current.max_lessons_to_inject, 5);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_updates_db_and_overlay_for_session_scoped_key() -> anyhow::Result<()> {
    mike::feature_settings::install();
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
    let write_lock = Arc::new(Mutex::new(SettingsWriteGuard));
    let svc =
        SettingsService::reconcile_on_startup(overlay.clone(), write_lock, handle.clone()).await?;

    let result = svc
        .set(
            "reminders.max_lessons_to_inject",
            serde_json::Value::Number(10u64.into()),
        )
        .await?;
    assert_eq!(result, serde_json::json!(10));

    let current = overlay.load().features.get::<RemindersOverlay>();
    assert_eq!(
        current.max_lessons_to_inject, 10,
        "overlay must reflect the flip before any DB re-read"
    );

    let rows = handle.fetch_settings().await?;
    let db_val = rows
        .iter()
        .find(|(k, _)| k == "reminders.max_lessons_to_inject")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| anyhow::anyhow!("row must exist in DB after upsert"))?;
    assert_eq!(db_val, serde_json::json!(10));

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_unknown_key_returns_not_found() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
    let write_lock = Arc::new(Mutex::new(SettingsWriteGuard));
    let svc = SettingsService::reconcile_on_startup(overlay, write_lock, handle).await?;

    let result = svc.set("no_such_setting", serde_json::json!(true)).await;
    assert!(
        matches!(result, Err(SettingsServiceError::NotFound(_))),
        "unknown key must return NotFound"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_rejects_secret_key_and_writes_no_row() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
    let write_lock = Arc::new(Mutex::new(SettingsWriteGuard));
    let svc = SettingsService::reconcile_on_startup(overlay, write_lock, handle.clone()).await?;

    let result = svc
        .set(
            "steerer.openrouter_api_key",
            serde_json::json!("sk-or-v1-must-not-persist"),
        )
        .await;
    assert!(
        matches!(result, Err(SettingsServiceError::InvalidValue { .. })),
        "a secret-shaped key must be refused, got {result:?}"
    );

    let rows = handle.fetch_settings().await?;
    assert!(
        rows.iter().all(|(k, v)| {
            k != "steerer.openrouter_api_key" && !v.to_string().contains("must-not-persist")
        }),
        "no settings row may hold the refused secret: {rows:?}"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_invalid_value_returns_invalid_value_error() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
    let write_lock = Arc::new(Mutex::new(SettingsWriteGuard));
    let svc = SettingsService::reconcile_on_startup(overlay, write_lock, handle).await?;

    let result = svc
        .set("reminders.max_lessons_to_inject", serde_json::json!(99i64))
        .await;
    assert!(
        matches!(result, Err(SettingsServiceError::InvalidValue { .. })),
        "out-of-range value must return InvalidValue"
    );

    db.cleanup().await?;
    Ok(())
}
