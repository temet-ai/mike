use std::time::Duration;

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::store::persistence::change_log::{NewChange, append_change_in};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy)]
pub struct LeaseClaim<'a> {
    pub role: &'a str,
    pub machine: Uuid,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct PublishRequest<'a> {
    pub pointer_key: &'a str,
    pub expected_bundle_id: i64,
    pub next_bundle_id: i64,
    pub lease: Option<LeaseClaim<'a>>,
    pub source_json: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Published,
    Cancelled,
    PointerMoved { current: i64 },
    LeaseLapsed,
}

fn pointer_of(value: &Value) -> i64 {
    value.as_i64().unwrap_or(0)
}

impl PersistHandle {
    pub async fn publish_live_pointer(
        &self,
        req: PublishRequest<'_>,
        cancel: &CancellationToken,
    ) -> Result<PublishOutcome, PersistError> {
        crate::metrics::time_db("publish_live_pointer", async move {
            if cancel.is_cancelled() {
                return Ok(PublishOutcome::Cancelled);
            }
            let mut tx = self.pool.begin().await?;
            if let Some(lease) = req.lease {
                let held = sqlx::query_scalar!(
                    r#"SELECT machine_uuid = $2
                              AND heartbeat_at >= now() - make_interval(secs => $3::float8)
                              AS "held!"
                         FROM llm.worker_leases WHERE role = $1 FOR SHARE"#,
                    lease.role,
                    lease.machine,
                    lease.timeout.as_secs_f64(),
                )
                .fetch_optional(&mut *tx)
                .await?;
                if held != Some(true) {
                    return Ok(PublishOutcome::LeaseLapsed);
                }
            }
            sqlx::query!(
                "INSERT INTO config.settings (key, value) VALUES ($1, '0'::jsonb)
                 ON CONFLICT (key) DO NOTHING",
                req.pointer_key
            )
            .execute(&mut *tx)
            .await?;
            let current = sqlx::query_scalar!(
                r#"SELECT value AS "value!: Value" FROM config.settings WHERE key = $1 FOR UPDATE"#,
                req.pointer_key
            )
            .fetch_one(&mut *tx)
            .await
            .map(|v| pointer_of(&v))?;
            if current != req.expected_bundle_id {
                return Ok(PublishOutcome::PointerMoved { current });
            }
            let next = json!(req.next_bundle_id);
            sqlx::query!(
                "UPDATE config.settings SET value = $2, updated_at = now() WHERE key = $1",
                req.pointer_key,
                next
            )
            .execute(&mut *tx)
            .await?;
            let old_promoted = sqlx::query_scalar!(
                "SELECT promoted_at FROM eval.bundles WHERE id = $1",
                req.expected_bundle_id
            )
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            let role = sqlx::query_scalar!(
                "UPDATE eval.bundles
                    SET promoted_at = COALESCE(promoted_at, $2::timestamptz, now())
                  WHERE id = $1
                RETURNING role",
                req.next_bundle_id,
                old_promoted
            )
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query!(
                "UPDATE eval.bundles
                    SET status = 'retired', superseded_at = now(), superseded_by_id = $2
                  WHERE id = $1 AND status IN ('live', 'live_ungated') AND id <> $2",
                req.expected_bundle_id,
                req.next_bundle_id
            )
            .execute(&mut *tx)
            .await?;
            let old_value = json!({"bundle_id": req.expected_bundle_id});
            let new_value = json!({"bundle_id": req.next_bundle_id});
            append_change_in(
                &mut tx,
                &NewChange {
                    subject_kind: "promotion",
                    subject_key: &role,
                    op: "update",
                    old_value: Some(&old_value),
                    new_value: Some(&new_value),
                    source_json: req.source_json,
                },
            )
            .await?;
            if cancel.is_cancelled() {
                return Ok(PublishOutcome::Cancelled);
            }
            tx.commit().await?;
            Ok(PublishOutcome::Published)
        })
        .await
    }
}
