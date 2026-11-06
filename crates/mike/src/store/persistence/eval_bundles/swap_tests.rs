use serde_json::{Value, json};

use super::super::test_support;
use super::{BuildOrigin, SwapRequest};
use crate::store::persistence::PersistHandle;

const SOURCE: &str = r#"{"via":"test","actor":"swap_tests"}"#;

async fn insert_bundle(
    persist: &PersistHandle,
    status: &str,
    components: &Value,
    promoted_at: Option<chrono::DateTime<chrono::Utc>>,
) -> anyhow::Result<i64> {
    let sha = crate::eval::bundles::content_sha256(components);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha, build_dirty, cause, promoted_at)
         VALUES ('turn_judge', '', 'candidate', $1, $2, $3, 'test', false, '{}'::jsonb, $4) RETURNING id",
    )
    .bind(status)
    .bind(components)
    .bind(sha)
    .bind(promoted_at)
    .fetch_one(persist.pool())
    .await?;
    Ok(id)
}

#[derive(sqlx::FromRow)]
struct Observed {
    status: String,
    components: Value,
    promoted_at: Option<chrono::DateTime<chrono::Utc>>,
    superseded_by_id: Option<i64>,
    cause: Value,
}

async fn observe(persist: &PersistHandle, id: i64) -> anyhow::Result<Observed> {
    Ok(sqlx::query_as(
        "SELECT status, components, promoted_at, superseded_by_id, cause FROM eval.bundles WHERE id = $1",
    )
    .bind(id)
    .fetch_one(persist.pool())
    .await?)
}

fn request<'a>(live: i64, model: &'a str) -> SwapRequest<'a> {
    SwapRequest {
        live_bundle_id: live,
        model,
        setting_key: "judge_worker.model",
        build: BuildOrigin {
            sha: "test",
            dirty: false,
        },
        source_json: SOURCE,
    }
}

#[tokio::test]
async fn swap_copies_the_live_bundle_with_only_the_model_changed_and_retires_it()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let promoted = chrono::Utc::now() - chrono::Duration::days(10);
    let components = json!({"model": "google/gemini-3.8-flash", "system_prompt": "judge", "rubric_version": "v2"});
    let live = insert_bundle(&persist, "live", &components, Some(promoted)).await?;

    let swap = persist
        .swap_live_bundle_model(request(live, "codex/gpt-6-sol"))
        .await?
        .ok_or_else(|| anyhow::anyhow!("expected a swap"))?;

    assert_eq!(swap.from_bundle_id, live);
    assert_eq!(swap.from_model, "google/gemini-3.8-flash");
    assert_eq!(swap.to_model, "codex/gpt-6-sol");
    let new = observe(&persist, swap.to_bundle_id).await?;
    assert_eq!(new.status, "live_ungated");
    assert_eq!(new.components["model"], "codex/gpt-6-sol");
    assert_eq!(new.components["system_prompt"], "judge");
    assert_eq!(new.components["rubric_version"], "v2");
    assert_eq!(
        new.promoted_at.map(|t| t.timestamp()),
        Some(promoted.timestamp())
    );
    assert_eq!(new.cause["copied_from_bundle"], json!(live));
    let old = observe(&persist, live).await?;
    assert_eq!(old.status, "retired");
    assert_eq!(old.superseded_by_id, Some(swap.to_bundle_id));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn swap_stamps_promoted_at_when_the_live_bundle_had_none() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let live = insert_bundle(&persist, "live_ungated", &json!({"model": "a"}), None).await?;
    let swap = persist
        .swap_live_bundle_model(request(live, "b"))
        .await?
        .ok_or_else(|| anyhow::anyhow!("expected a swap"))?;
    assert!(
        observe(&persist, swap.to_bundle_id)
            .await?
            .promoted_at
            .is_some()
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn swap_does_nothing_for_the_same_model_or_a_retired_bundle() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let live = insert_bundle(&persist, "live", &json!({"model": "a"}), None).await?;
    assert!(
        persist
            .swap_live_bundle_model(request(live, "a"))
            .await?
            .is_none()
    );
    let retired = insert_bundle(&persist, "retired", &json!({"model": "a"}), None).await?;
    assert!(
        persist
            .swap_live_bundle_model(request(retired, "b"))
            .await?
            .is_none()
    );
    assert!(
        persist
            .swap_live_bundle_model(request(-1, "b"))
            .await?
            .is_none()
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn retire_bundle_retires_only_a_live_bundle_once() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let previous = insert_bundle(&persist, "live", &json!({"model": "a"}), None).await?;
    let next = insert_bundle(&persist, "live_ungated", &json!({"model": "b"}), None).await?;
    assert!(persist.retire_bundle(previous, next, SOURCE).await?);
    assert!(!persist.retire_bundle(previous, next, SOURCE).await?);
    assert!(!persist.retire_bundle(next, next, SOURCE).await?);
    let old = observe(&persist, previous).await?;
    assert_eq!(old.status, "retired");
    assert_eq!(old.superseded_by_id, Some(next));
    db.cleanup().await?;
    Ok(())
}
