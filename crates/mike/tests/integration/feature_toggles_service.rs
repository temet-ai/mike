use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;

use crate::test_db::TestDb;

use mike::config::FeatureToggleWriteGuard;
use mike::feature_toggles::service::{FeatureToggleService, FeatureToggleServiceError};
use mike::feature_toggles::{FeatureToggles, TOGGLE_META, get_toggle_enabled};
use mike::store::persistence::PersistHandle;

#[tokio::test]
async fn reconcile_seeds_overlay_from_db() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let overlay = Arc::new(ArcSwap::from_pointee(FeatureToggles::default()));
    let write_lock = Arc::new(Mutex::new(FeatureToggleWriteGuard));
    let _svc =
        FeatureToggleService::reconcile_on_startup(overlay.clone(), write_lock, handle).await?;

    let current = overlay.load();
    let defaults = FeatureToggles::default();
    for meta in TOGGLE_META {
        let enabled = get_toggle_enabled(&current, meta.key);
        let expected = get_toggle_enabled(&defaults, meta.key);
        assert_eq!(
            enabled, expected,
            "a fresh install must reconcile '{}' to its code default",
            meta.key
        );
    }

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reconcile_prefers_code_default_when_not_operator_set() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let raw_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&db.database_url)
        .await?;
    sqlx::query!("UPDATE config.feature_toggles SET operator_set = false WHERE key = 'reminders'")
        .execute(&raw_pool)
        .await?;
    raw_pool.close().await;

    let overlay = Arc::new(ArcSwap::from_pointee(FeatureToggles::default()));
    let write_lock = Arc::new(Mutex::new(FeatureToggleWriteGuard));
    let _svc =
        FeatureToggleService::reconcile_on_startup(overlay.clone(), write_lock, handle.clone())
            .await?;

    assert_eq!(
        overlay.load().reminders,
        FeatureToggles::default().reminders,
        "an untouched row must read as the code default, not the stored enabled=true"
    );

    handle.upsert_feature_toggle("reminders", true).await?;
    let rows = handle.fetch_feature_toggles().await?;
    let operator_set = rows
        .iter()
        .find(|(k, _, _)| k == "reminders")
        .map(|(_, _, o)| *o)
        .ok_or_else(|| anyhow::anyhow!("reminders row must exist"))?;
    assert!(operator_set, "a settings write must set operator_set");

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_updates_db_and_overlay() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let overlay = Arc::new(ArcSwap::from_pointee(FeatureToggles::default()));
    let write_lock = Arc::new(Mutex::new(FeatureToggleWriteGuard));
    let svc =
        FeatureToggleService::reconcile_on_startup(overlay.clone(), write_lock, handle.clone())
            .await?;

    let (key, enabled) = svc.set("reminders", false).await?;
    assert_eq!(key, "reminders");
    assert!(!enabled);

    let current = overlay.load();
    assert!(
        !current.reminders,
        "overlay must reflect the flip before any DB re-read"
    );

    let db_rows = handle.fetch_feature_toggles().await?;
    let db_reminders = db_rows
        .iter()
        .find(|(k, _, _)| k == "reminders")
        .map(|(_, e, _)| *e)
        .ok_or_else(|| anyhow::anyhow!("reminders row must exist in DB after upsert"))?;
    assert!(!db_reminders, "DB must reflect the flip");

    let overlay2 = Arc::new(ArcSwap::from_pointee(FeatureToggles::default()));
    let write_lock2 = Arc::new(Mutex::new(FeatureToggleWriteGuard));
    let _svc2 =
        FeatureToggleService::reconcile_on_startup(overlay2.clone(), write_lock2, handle.clone())
            .await?;
    let current2 = overlay2.load();
    assert!(
        !current2.reminders,
        "after restart, reconcile must restore the DB-persisted false"
    );

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

    let overlay = Arc::new(ArcSwap::from_pointee(FeatureToggles::default()));
    let write_lock = Arc::new(Mutex::new(FeatureToggleWriteGuard));
    let svc = FeatureToggleService::reconcile_on_startup(overlay, write_lock, handle).await?;

    let result = svc.set("no_such_feature", true).await;
    assert!(
        matches!(result, Err(FeatureToggleServiceError::NotFound(_))),
        "unknown key must return NotFound"
    );

    db.cleanup().await?;
    Ok(())
}
