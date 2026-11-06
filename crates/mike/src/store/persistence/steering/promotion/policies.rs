use serde_json::json;

use super::super::super::change_log::{NewChange, append_change_in};
use super::super::super::{PersistError, PersistHandle};
use super::{ModeChange, PolicyRow};

async fn toggle_steerer(
    tx: &mut sqlx::PgConnection,
    source_json: &str,
    steerer_id: Option<i64>,
    enabled: bool,
) -> Result<(), PersistError> {
    let Some(id) = steerer_id else {
        return Ok(());
    };
    super::super::super::change_log::set_change_source(tx, source_json).await?;
    sqlx::query!(
        "UPDATE steering.steerers SET enabled = $2, updated_at = now()
          WHERE id = $1 AND enabled <> $2",
        id,
        enabled,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

impl PersistHandle {
    pub async fn policy_rows(&self, steerer_id: i64) -> Result<Vec<PolicyRow>, PersistError> {
        crate::metrics::time_db("policy_rows", async move {
            Ok(sqlx::query_as!(
                PolicyRow,
                "SELECT id, steerer_id, question, mode, holdback_pct, holdback_salt, updated_at,
                        updated_via
                   FROM steering.question_policies WHERE steerer_id = $1 ORDER BY question",
                steerer_id,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn policy_row(
        &self,
        steerer_id: i64,
        question: &str,
    ) -> Result<Option<PolicyRow>, PersistError> {
        crate::metrics::time_db("policy_row", async move {
            Ok(sqlx::query_as!(
                PolicyRow,
                "SELECT id, steerer_id, question, mode, holdback_pct, holdback_salt, updated_at,
                        updated_via
                   FROM steering.question_policies WHERE steerer_id = $1 AND question = $2",
                steerer_id,
                question,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn apply_mode_change(&self, c: &ModeChange<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("apply_mode_change", async move {
            let mut tx = self.pool.begin().await?;
            let updated = sqlx::query!(
                "UPDATE steering.question_policies
                    SET mode = $2, updated_via = $3, updated_at = now()
                  WHERE id = $1 AND mode = $4",
                c.policy_id,
                c.to_mode,
                c.decided_via,
                c.from_mode,
            )
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(PersistError::DataInvariant(format!(
                    "policy {} is no longer in mode {}",
                    c.policy_id, c.from_mode
                )));
            }
            toggle_steerer(&mut tx, c.source_json, c.replaces_steerer_id, false).await?;
            toggle_steerer(&mut tx, c.source_json, c.restored_steerer_id, true).await?;
            let subject = format!("turn-guard/{}", c.question);
            let old = json!(c.from_mode);
            let new = json!(c.to_mode);
            let ids = append_change_in(
                &mut tx,
                &NewChange {
                    subject_kind: "promotion",
                    subject_key: &subject,
                    op: "update",
                    old_value: Some(&old),
                    new_value: Some(&new),
                    source_json: c.source_json,
                },
            )
            .await?;
            let id = sqlx::query_scalar!(
                r#"INSERT INTO steering.question_promotions
                       (policy_id, from_mode, to_mode, action, decided_via, gate_passed, gate_json,
                        override_reason, replaces_steerer_id, restored_steerer_id, change_log_id)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                   RETURNING id AS "id!""#,
                c.policy_id,
                c.from_mode,
                c.to_mode,
                c.action,
                c.decided_via,
                c.gate_passed,
                c.gate_json,
                c.override_reason,
                c.replaces_steerer_id,
                c.restored_steerer_id,
                ids.last().copied(),
            )
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(id)
        })
        .await
    }

    pub async fn set_steerer_enabled_preserving_config(
        &self,
        steerer_id: i64,
        enabled: bool,
        source_json: &str,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("set_steerer_enabled_preserving_config", async move {
            let mut tx = self.pool.begin().await?;
            toggle_steerer(&mut tx, source_json, Some(steerer_id), enabled).await?;
            tx.commit().await?;
            Ok(())
        })
        .await
    }

    pub async fn set_holdback_pct(
        &self,
        steerer_id: i64,
        question: &str,
        pct: Option<i16>,
        via: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("set_holdback_pct", async move {
            let r = sqlx::query!(
                "UPDATE steering.question_policies
                    SET holdback_pct = $3, updated_at = now(), updated_via = $4
                  WHERE steerer_id = $1 AND question = $2",
                steerer_id,
                question,
                pct,
                via,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected() > 0)
        })
        .await
    }
}
