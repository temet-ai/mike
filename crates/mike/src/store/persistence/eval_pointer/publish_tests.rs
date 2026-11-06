use std::time::Duration;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{LeaseClaim, PublishOutcome, PublishRequest};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support;

const KEY: &str = "turn_judge.live_bundle_id";
const SOURCE: &str = r#"{"via":"test","actor":"publish_tests"}"#;
const ROLE: &str = "eval";

async fn bundle(persist: &PersistHandle, status: &str, tag: &str) -> anyhow::Result<i64> {
    let components = serde_json::json!({"model": tag});
    let sha = crate::eval::bundles::content_sha256(&components);
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha, build_dirty, cause)
         VALUES ('turn_judge', '', 'candidate', $1, $2, $3, 'test', false, '{}'::jsonb) RETURNING id",
    )
    .bind(status)
    .bind(components)
    .bind(sha)
    .fetch_one(persist.pool())
    .await?)
}

async fn set_pointer(persist: &PersistHandle, id: i64) -> anyhow::Result<()> {
    persist.upsert_setting(KEY, &serde_json::json!(id)).await?;
    Ok(())
}

async fn pointer(persist: &PersistHandle) -> anyhow::Result<i64> {
    let value = persist.setting_value(KEY).await?;
    Ok(value.and_then(|v| v.as_i64()).unwrap_or(0))
}

async fn status_of(persist: &PersistHandle, id: i64) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT status FROM eval.bundles WHERE id = $1")
            .bind(id)
            .fetch_one(persist.pool())
            .await?,
    )
}

fn request<'a>(expected: i64, next: i64, lease: Option<LeaseClaim<'a>>) -> PublishRequest<'a> {
    PublishRequest {
        pointer_key: KEY,
        expected_bundle_id: expected,
        next_bundle_id: next,
        lease,
        source_json: SOURCE,
    }
}

#[tokio::test]
async fn a_publication_moves_the_pointer_and_retires_the_old_bundle_together() -> anyhow::Result<()>
{
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let old = bundle(&persist, "live", "old").await?;
    let new = bundle(&persist, "candidate", "new").await?;
    set_pointer(&persist, old).await?;
    let outcome = persist
        .publish_live_pointer(request(old, new, None), &CancellationToken::new())
        .await?;
    assert_eq!(outcome, PublishOutcome::Published);
    assert_eq!(pointer(&persist).await?, new);
    assert_eq!(status_of(&persist, old).await?, "retired");
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_cancelled_publication_leaves_the_old_pointer_and_bundle() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let old = bundle(&persist, "live", "old").await?;
    let new = bundle(&persist, "candidate", "new").await?;
    set_pointer(&persist, old).await?;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let outcome = persist
        .publish_live_pointer(request(old, new, None), &cancel)
        .await?;
    assert_eq!(outcome, PublishOutcome::Cancelled);
    assert_eq!(pointer(&persist).await?, old);
    assert_eq!(status_of(&persist, old).await?, "live");
    assert_eq!(status_of(&persist, new).await?, "candidate");
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_worker_whose_lease_lapsed_cannot_overwrite_a_newer_publication() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let first = bundle(&persist, "live", "first").await?;
    let stale = bundle(&persist, "candidate", "stale").await?;
    let newer = bundle(&persist, "candidate", "newer").await?;
    set_pointer(&persist, first).await?;
    let timeout = Duration::from_secs(180);
    let slow = Uuid::new_v4();
    let fast = Uuid::new_v4();
    assert!(persist.claim_llm_worker_lease(ROLE, slow, timeout).await?);
    sqlx::query(
        "UPDATE llm.worker_leases SET heartbeat_at = now() - interval '1 hour' WHERE role = $1",
    )
    .bind(ROLE)
    .execute(persist.pool())
    .await?;
    assert!(persist.claim_llm_worker_lease(ROLE, fast, timeout).await?);
    let cancel = CancellationToken::new();
    let claim = |machine| LeaseClaim {
        role: ROLE,
        machine,
        timeout,
    };
    let published = persist
        .publish_live_pointer(request(first, newer, Some(claim(fast))), &cancel)
        .await?;
    assert_eq!(published, PublishOutcome::Published);
    let lapsed = persist
        .publish_live_pointer(request(first, stale, Some(claim(slow))), &cancel)
        .await?;
    assert_eq!(lapsed, PublishOutcome::LeaseLapsed);
    let moved = persist
        .publish_live_pointer(request(first, stale, Some(claim(fast))), &cancel)
        .await?;
    assert_eq!(moved, PublishOutcome::PointerMoved { current: newer });
    assert_eq!(pointer(&persist).await?, newer);
    assert_eq!(status_of(&persist, stale).await?, "candidate");
    db.cleanup().await?;
    Ok(())
}
