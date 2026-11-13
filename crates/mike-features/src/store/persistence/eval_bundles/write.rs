use serde_json::json;

use super::super::PersistError;
use super::super::change_log::{NewChange, append_change_in};

#[derive(Debug, Clone, PartialEq)]
pub struct NewBundleRow {
    pub role: String,
    pub subject_key: String,
    pub components: serde_json::Value,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct EnsuredBundle {
    pub id: i64,
    pub role: String,
    pub subject_key: String,
}

#[derive(Debug, Clone, Copy)]
pub struct BuildOrigin<'a> {
    pub sha: &'a str,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct StatusChange<'a> {
    pub id: i64,
    pub status: &'a str,
    pub gate_run_id: Option<i64>,
    pub override_reason: Option<&'a str>,
    pub source_json: &'a str,
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn ensure_bundles(
        self,
        candidates: &[NewBundleRow],
        build: BuildOrigin<'_>,
        cause: &serde_json::Value,
    ) -> Result<Vec<EnsuredBundle>, PersistError> {
        let roles: Vec<String> = candidates.iter().map(|c| c.role.clone()).collect();
        let subjects: Vec<String> = candidates.iter().map(|c| c.subject_key.clone()).collect();
        let components: Vec<serde_json::Value> =
            candidates.iter().map(|c| c.components.clone()).collect();
        let hashes: Vec<String> = candidates
            .iter()
            .map(|c| c.content_sha256.clone())
            .collect();
        crate::metrics::time_db("ensure_bundles", async move {
            let rows = sqlx::query_file_as!(
                EnsuredBundle,
                "src/store/persistence/eval_bundles/ensure.sql",
                &roles,
                &subjects,
                &components,
                &hashes,
                build.sha,
                build.dirty,
                cause,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn create_candidate_bundle(
        self,
        role: &str,
        subject_key: &str,
        components: &serde_json::Value,
    ) -> Result<i64, PersistError> {
        let hash = crate::eval::bundles::content_sha256(components);
        let build = crate::change_log::current_build();
        crate::metrics::time_db("create_candidate_bundle", async move {
            let id = sqlx::query_file_scalar!(
                "src/store/persistence/eval_bundles/candidate.sql",
                role,
                subject_key,
                components,
                &hash,
                build.sha,
                build.dirty,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn set_bundle_status(
        self,
        change: &StatusChange<'_>,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("set_bundle_status", async move {
            let mut tx = self.pool.begin().await?;
            let current = sqlx::query!(
                "SELECT role, subject_key, status FROM eval.bundles WHERE id = $1 FOR UPDATE",
                change.id,
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(current) = current else {
                return Err(PersistError::DataInvariant(format!(
                    "no eval.bundles row with id {}",
                    change.id
                )));
            };
            let (role, subject_key, old_status) =
                (current.role, current.subject_key, current.status);
            sqlx::query!(
                "UPDATE eval.bundles
    SET status = $2,
        gate_run_id = COALESCE($3, gate_run_id),
        override_reason = COALESCE($4, override_reason),
        promoted_at = CASE WHEN $2 = 'live' THEN now()
                           WHEN $2 = 'live_ungated' THEN COALESCE(promoted_at, now())
                           ELSE promoted_at END
  WHERE id = $1",
                change.id,
                change.status,
                change.gate_run_id,
                change.override_reason,
            )
            .execute(&mut *tx)
            .await?;
            let key = if subject_key.is_empty() {
                role.clone()
            } else {
                format!("{role}:{subject_key}")
            };
            let old_value = json!({"bundle_id": change.id, "status": old_status});
            let new_value = json!({
                "bundle_id": change.id,
                "status": change.status,
                "gate_run_id": change.gate_run_id,
                "override_reason": change.override_reason,
            });
            let ids = append_change_in(
                &mut tx,
                &NewChange {
                    subject_kind: "promotion",
                    subject_key: &key,
                    op: "update",
                    old_value: Some(&old_value),
                    new_value: Some(&new_value),
                    source_json: change.source_json,
                },
            )
            .await?;
            tx.commit().await?;
            Ok(ids)
        })
        .await
    }
}
