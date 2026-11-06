use super::{InterventionStaleness, NewIntervention};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn insert_intervention(&self, new: NewIntervention<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_intervention", async move {
            let mut tx = self.pool.begin().await?;
            let id: i64 = sqlx::query_scalar(
                r#"INSERT INTO steering.interventions
                       (steerer_id, enrolment_id, session_id,
                        verdict_id, mode_id, authority_id,
                        trigger_request_id, fired_response_id,
                        confidence, off_thread_topic, payload_text,
                        missing_evidence, model, anchor_text, uuid, expires_at,
                        verdict_json, operator_turn_id, bundle_id,
                        delivery_channel, addressed_to, deliver_after, wake_state)
                   SELECT $1, $2, $3,
                          v.id, m.id, a.id,
                          $7, r.id,
                          $8, $9, $10,
                          $11, $12, $13, $14, $15,
                          $16, $17, $18,
                          $19, $20, $21, $22
                     FROM steering.verdicts     v
                     JOIN steering.modes        m ON m.code = $5
                     JOIN steering.authorities  a ON a.code = $6
                     LEFT JOIN responses.responses r ON r.request_id = $7
                    WHERE v.code = $4
                   RETURNING id"#,
            )
            .bind(new.steerer_id)
            .bind(new.enrolment_id)
            .bind(new.session_id)
            .bind(new.verdict_code)
            .bind(new.mode_code)
            .bind(new.authority_code)
            .bind(new.trigger_request_id)
            .bind(new.confidence)
            .bind(new.off_thread_topic)
            .bind(new.payload_text)
            .bind(new.missing_evidence)
            .bind(new.model)
            .bind(new.anchor_text)
            .bind(new.intervention_uuid)
            .bind(new.expires_at)
            .bind(new.verdict_json)
            .bind(new.operator_turn_id)
            .bind(new.bundle_id)
            .bind(new.delivery_channel)
            .bind(new.addressed_to)
            .bind(new.deliver_after)
            .bind(new.wake_state)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(id)
        })
        .await
    }

    pub(crate) async fn has_intervention_for_turn(
        &self,
        steerer_id: i64,
        operator_turn_id: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("has_intervention_for_turn", async move {
            let exists = sqlx::query_scalar!(
                r#"SELECT EXISTS(
                       SELECT 1 FROM steering.interventions
                        WHERE steerer_id = $1 AND operator_turn_id = $2
                   ) AS "exists!""#,
                steerer_id,
                operator_turn_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(exists)
        })
        .await
    }

    pub(crate) async fn intervention_staleness(
        &self,
        intervention_uuid: uuid::Uuid,
    ) -> Result<Option<InterventionStaleness>, PersistError> {
        crate::metrics::time_db("intervention_staleness", async move {
            let row = sqlx::query!(
                r#"SELECT i.expires_at, i.pushed_at, i.acked_at,
                          (sub.id IS NOT NULL) AS "superseded!"
                     FROM steering.interventions i
                     LEFT JOIN LATERAL (
                         SELECT r.id
                           FROM requests.requests r
                          WHERE r.session_id = i.session_id
                            AND r.id > i.trigger_request_id
                            AND r.human_prompt <> ''
                            AND r.turn_origin_id IN (1, 6)
                            AND r.human_prompt NOT LIKE '<channel %'
                            AND r.human_prompt NOT LIKE '%<task-notification>%'
                            AND r.human_prompt NOT LIKE '%<teammate-message%'
                            AND r.human_prompt NOT LIKE '%<agent-message from=%'
                            AND r.human_prompt NOT LIKE '%Stop hook feedback:%'
                            AND r.human_prompt NOT LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
                            AND r.human_prompt NOT LIKE 'Tool loaded.%'
                            AND r.human_prompt NOT LIKE '%<<autonomous-loop-dynamic>>%'
                            AND r.human_prompt NOT LIKE '%<<autonomous-loop>>%'
                            AND r.human_prompt NOT LIKE '%Fleet coordinator cycle wake%'
                            AND r.human_prompt NOT LIKE '%Cross-session message from%'
                            AND r.human_prompt NOT LIKE '%The user stepped away and is coming back%'
                            AND r.human_prompt NOT LIKE '%Steering note from the operator''s proxy:%'
                          LIMIT 1
                     ) sub ON true
                    WHERE i.uuid = $1"#,
                intervention_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| InterventionStaleness {
                expires_at: r.expires_at,
                superseded: r.superseded,
                pushed_at: r.pushed_at,
                acked_at: r.acked_at,
            }))
        })
        .await
    }

    pub(crate) async fn expire_intervention(
        &self,
        intervention_uuid: uuid::Uuid,
        reason: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("expire_intervention", async move {
            let rows_affected = sqlx::query!(
                r#"UPDATE steering.interventions
                      SET reverted_at   = NOW(),
                          revert_reason = $2
                    WHERE uuid            = $1
                      AND reverted_at IS NULL"#,
                intervention_uuid,
                reason,
            )
            .execute(&self.pool)
            .await?
            .rows_affected();
            Ok(rows_affected > 0)
        })
        .await
    }

    pub(crate) async fn revert_intervention(
        &self,
        id: i64,
        reason: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("revert_intervention", async move {
            let rows_affected = sqlx::query!(
                r#"UPDATE steering.interventions
                      SET reverted_at   = NOW(),
                          revert_reason = $2
                    WHERE id             = $1
                      AND reverted_at IS NULL"#,
                id,
                reason,
            )
            .execute(&self.pool)
            .await?
            .rows_affected();
            Ok(rows_affected > 0)
        })
        .await
    }
}

#[cfg(test)]
mod machine_text_staleness_tests;
