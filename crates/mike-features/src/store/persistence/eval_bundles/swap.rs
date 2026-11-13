use serde_json::{Value, json};

use super::super::PersistError;
use super::super::change_log::{NewChange, append_change_in};
use super::BuildOrigin;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LiveModelSwap {
    pub role: String,
    pub from_bundle_id: i64,
    pub to_bundle_id: i64,
    pub from_model: String,
    pub to_model: String,
}

#[derive(Debug, Clone, Copy)]
pub struct SwapRequest<'a> {
    pub live_bundle_id: i64,
    pub model: &'a str,
    pub setting_key: &'a str,
    pub build: BuildOrigin<'a>,
    pub source_json: &'a str,
}

struct LockedBundle {
    role: String,
    subject_key: String,
    status: String,
    components: Value,
    promoted_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn copied_components(components: &Value, model: &str) -> Value {
    let mut next = components.clone();
    if let Some(obj) = next.as_object_mut() {
        obj.insert("model".to_owned(), Value::String(model.to_owned()));
    }
    next
}

fn model_of(components: &Value) -> String {
    components
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn is_live(status: &str) -> bool {
    matches!(status, "live" | "live_ungated")
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn swap_live_bundle_model(
        self,
        req: SwapRequest<'_>,
    ) -> Result<Option<LiveModelSwap>, PersistError> {
        crate::metrics::time_db("swap_live_bundle_model", async move {
            let mut tx = self.pool.begin().await?;
            let locked = sqlx::query_as!(
                LockedBundle,
                "SELECT role, subject_key, status, components, promoted_at
                   FROM eval.bundles WHERE id = $1 FOR UPDATE",
                req.live_bundle_id
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(old) = locked else {
                return Ok(None);
            };
            let from_model = model_of(&old.components);
            if from_model == req.model || !is_live(&old.status) {
                return Ok(None);
            }
            let components = copied_components(&old.components, req.model);
            let sha = crate::eval::bundles::content_sha256(&components);
            let cause = json!({
                "trigger": "setting",
                "keys": [req.setting_key],
                "copied_from_bundle": req.live_bundle_id,
                "changed": ["model"],
            });
            let reason = format!(
                "{} changed to {} while bundle {} was live; this bundle is that one with only the model replaced, ungated",
                req.setting_key, req.model, req.live_bundle_id
            );
            let to_bundle_id = sqlx::query_scalar!(
                "INSERT INTO eval.bundles
                    (role, subject_key, origin, status, components, content_sha256, build_sha,
                     build_dirty, cause, promoted_at, override_reason)
                 VALUES ($1, $2, 'candidate', 'live_ungated', $3, $4, $5, $6, $7,
                         COALESCE($8::timestamptz, now()), $9)
                 RETURNING id",
                old.role,
                old.subject_key,
                components,
                sha,
                req.build.sha,
                req.build.dirty,
                cause,
                old.promoted_at,
                reason
            )
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query!(
                "UPDATE eval.bundles
                    SET status = 'retired', superseded_at = now(), superseded_by_id = $2
                  WHERE id = $1",
                req.live_bundle_id,
                to_bundle_id
            )
            .execute(&mut *tx)
            .await?;
            let old_value = json!({"bundle_id": req.live_bundle_id, "model": from_model});
            let new_value = json!({"bundle_id": to_bundle_id, "model": req.model});
            append_change_in(
                &mut tx,
                &NewChange {
                    subject_kind: "promotion",
                    subject_key: &old.role,
                    op: "update",
                    old_value: Some(&old_value),
                    new_value: Some(&new_value),
                    source_json: req.source_json,
                },
            )
            .await?;
            tx.commit().await?;
            Ok(Some(LiveModelSwap {
                role: old.role,
                from_bundle_id: req.live_bundle_id,
                to_bundle_id,
                from_model,
                to_model: req.model.to_owned(),
            }))
        })
        .await
    }

    pub async fn retire_bundle(
        self,
        bundle_id: i64,
        superseded_by_id: i64,
        source_json: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("retire_bundle", async move {
            let mut tx = self.pool.begin().await?;
            let role = sqlx::query_scalar!(
                "UPDATE eval.bundles
                    SET status = 'retired', superseded_at = now(), superseded_by_id = $2
                  WHERE id = $1 AND status IN ('live', 'live_ungated') AND id <> $2
                 RETURNING role",
                bundle_id,
                superseded_by_id
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(role) = role else {
                return Ok(false);
            };
            let old_value = json!({"bundle_id": bundle_id, "status": "live"});
            let new_value = json!({"bundle_id": bundle_id, "status": "retired", "superseded_by_id": superseded_by_id});
            append_change_in(
                &mut tx,
                &NewChange {
                    subject_kind: "promotion",
                    subject_key: &role,
                    op: "update",
                    old_value: Some(&old_value),
                    new_value: Some(&new_value),
                    source_json,
                },
            )
            .await?;
            tx.commit().await?;
            Ok(true)
        })
        .await
    }
}
