use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::super::service::SettingsService;
use super::super::{SettingsOverlay, meta_session_end, meta_turn_judge};
use crate::change_log::ChangeSource;
use crate::config::SettingsWriteGuard;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support;

async fn insert_live_bundle(
    persist: &PersistHandle,
    role: &str,
    model: &str,
) -> anyhow::Result<i64> {
    let components = json!({"model": model, "system_prompt": "judge"});
    let sha = crate::eval::bundles::content_sha256(&components);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha, build_dirty, cause, promoted_at)
         VALUES ($1, '', 'candidate', 'live', $2, $3, 'test', false, '{}'::jsonb, now()) RETURNING id",
    )
    .bind(role)
    .bind(&components)
    .bind(sha)
    .fetch_one(persist.pool())
    .await?;
    Ok(id)
}

async fn bundle_model(persist: &PersistHandle, id: i64) -> anyhow::Result<String> {
    let components: Value = sqlx::query_scalar("SELECT components FROM eval.bundles WHERE id = $1")
        .bind(id)
        .fetch_one(persist.pool())
        .await?;
    Ok(components["model"].as_str().unwrap_or_default().to_owned())
}

async fn service_for(persist: &PersistHandle) -> anyhow::Result<Arc<SettingsService>> {
    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
    SettingsService::reconcile_on_startup(
        overlay,
        Arc::new(Mutex::new(SettingsWriteGuard)),
        persist.clone(),
    )
    .await
}

#[tokio::test]
async fn provider_change_moves_both_live_judge_bundles_to_the_provider_tier_models()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let turn = insert_live_bundle(&persist, "turn_judge", "google/gemini-3.8-flash").await?;
    let session =
        insert_live_bundle(&persist, "session_judge", "google/gemini-3.5-flash-lite").await?;
    let service = service_for(&persist).await?;
    service
        .set(meta_turn_judge::LIVE_BUNDLE_ID_KEY, json!(turn))
        .await?;
    service
        .set(meta_session_end::LIVE_BUNDLE_ID_KEY, json!(session))
        .await?;
    service.set("llm.provider", json!("claude")).await?;

    let swaps = service
        .follow_provider_change(&ChangeSource::internal())
        .await;

    assert_eq!(swaps.len(), 2, "{swaps:?}");
    let turn_swap = swaps
        .iter()
        .find(|s| s.from_bundle_id == turn)
        .ok_or_else(|| anyhow::anyhow!("no turn judge swap"))?;
    assert_eq!(turn_swap.to_model, "claude/sonnet");
    assert_eq!(
        bundle_model(&persist, turn_swap.to_bundle_id).await?,
        "claude/sonnet"
    );
    let session_swap = swaps
        .iter()
        .find(|s| s.from_bundle_id == session)
        .ok_or_else(|| anyhow::anyhow!("no session judge swap"))?;
    assert_eq!(session_swap.to_model, "claude/haiku");
    assert_eq!(
        service.live_pointer(meta_turn_judge::LIVE_BUNDLE_ID_KEY),
        Some(turn_swap.to_bundle_id)
    );
    assert_eq!(
        service.live_pointer(meta_session_end::LIVE_BUNDLE_ID_KEY),
        Some(session_swap.to_bundle_id)
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn returning_to_per_role_moves_the_live_bundle_back_to_the_roles_own_model()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let turn = insert_live_bundle(&persist, "turn_judge", "google/gemini-3.8-flash").await?;
    let service = service_for(&persist).await?;
    service
        .set(meta_turn_judge::LIVE_BUNDLE_ID_KEY, json!(turn))
        .await?;
    service
        .set("judge_worker.model", json!("google/own-pick"))
        .await?;
    service.set("llm.provider", json!("codex")).await?;
    let to_codex = service
        .follow_provider_change(&ChangeSource::internal())
        .await;
    assert_eq!(
        to_codex.first().map(|s| s.to_model.as_str()),
        Some("codex/gpt-6-sol")
    );

    service.set("llm.provider", json!("per_role")).await?;
    let back = service
        .follow_provider_change(&ChangeSource::internal())
        .await;

    assert_eq!(
        back.first().map(|s| s.to_model.as_str()),
        Some("google/own-pick")
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn provider_change_without_a_live_bundle_swaps_nothing() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let service = service_for(&persist).await?;
    service.set("llm.provider", json!("claude")).await?;
    let swaps = service
        .follow_provider_change(&ChangeSource::internal())
        .await;
    assert!(swaps.is_empty(), "{swaps:?}");
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_per_role_model_change_under_a_provider_keeps_the_bundle_on_the_provider_model()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let turn = insert_live_bundle(&persist, "turn_judge", "google/gemini-3.8-flash").await?;
    let service = service_for(&persist).await?;
    service
        .set(meta_turn_judge::LIVE_BUNDLE_ID_KEY, json!(turn))
        .await?;
    service.set("llm.provider", json!("claude")).await?;
    let first = service
        .follow_provider_change(&ChangeSource::internal())
        .await;
    assert_eq!(first.len(), 1);

    service
        .set("judge_worker.model", json!("google/other-pick"))
        .await?;
    let again = service
        .follow_model_change(
            "judge_worker.model",
            "google/other-pick",
            &ChangeSource::internal(),
        )
        .await;

    assert!(again.is_none(), "{again:?}");
    db.cleanup().await?;
    Ok(())
}
