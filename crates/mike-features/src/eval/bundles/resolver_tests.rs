use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;

use super::BundleCause;
use super::resolver::{BundlePublisher, BundleResolver, BundleStaticInputs};
use crate::config::SettingsWriteGuard;
use crate::settings::SettingsOverlay;
use crate::steerer::registry::SteererRegistry;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support;

fn publisher(persist: &PersistHandle, smoke_run: bool) -> BundlePublisher {
    let inputs = BundleStaticInputs {
        judge: crate::config::steerer::JudgeConfig::default(),
        session_judge:
            crate::features_settings::sections::session_judge::SessionJudgeConfig::default(),
        fleet_default_model: "openai/gpt-test".to_owned(),
        smoke_run,
    };
    let registry = Arc::new(ArcSwap::from_pointee(SteererRegistry::default()));
    let resolver = BundleResolver::new(persist.clone(), inputs, registry);
    BundlePublisher::new(
        resolver,
        Arc::new(ArcSwap::from_pointee(SettingsOverlay::default())),
        Arc::new(Mutex::new(SettingsWriteGuard)),
    )
}

async fn bundle_count(persist: &PersistHandle) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM eval.bundles")
        .fetch_one(persist.pool())
        .await?)
}

#[tokio::test]
async fn a_smoke_run_mints_no_bundle() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let smoke = publisher(&persist, true);
    let ids = smoke
        .resolve_ids(&SettingsOverlay::default(), BundleCause::startup())
        .await;
    assert!(ids.turn_judge.is_none());
    assert_eq!(bundle_count(&persist).await?, 0);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_role_with_a_live_pointer_mints_no_bundle_from_compiled_defaults() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let live = publisher(&persist, false);
    let overlay = SettingsOverlay {
        turn_judge_live_bundle_id: 501,
        session_judge_live_bundle_id: 502,
        ..Default::default()
    };
    let ids = live
        .resolve_ids(&overlay, BundleCause::setting("judge_worker.model", vec![]))
        .await;
    assert_eq!(ids.turn_judge, Some(501));
    assert_eq!(ids.session_judge, Some(502));
    assert_eq!(bundle_count(&persist).await?, 0);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn an_unchanged_configuration_reuses_its_bundles() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let live = publisher(&persist, false);
    let overlay = SettingsOverlay::default();
    let first = live.resolve_ids(&overlay, BundleCause::startup()).await;
    let second = live.resolve_ids(&overlay, BundleCause::startup()).await;
    assert!(first.turn_judge.is_some() && first.session_judge.is_some());
    assert_eq!(first, second);
    assert_eq!(bundle_count(&persist).await?, 2);
    let changed = SettingsOverlay {
        judge_worker_model: Some("openai/other".to_owned()),
        ..Default::default()
    };
    let third = live
        .resolve_ids(&changed, BundleCause::setting("judge_worker.model", vec![]))
        .await;
    assert_ne!(third.turn_judge, first.turn_judge);
    assert_eq!(third.session_judge, first.session_judge);
    let superseded: Option<i64> =
        sqlx::query_scalar("SELECT superseded_by_id FROM eval.bundles WHERE id = $1")
            .bind(first.turn_judge)
            .fetch_one(persist.pool())
            .await?;
    assert_eq!(superseded, third.turn_judge);
    db.cleanup().await?;
    Ok(())
}
