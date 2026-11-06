use super::flow_capture_admission;
use super::{PersistError, PersistHandle};
use super::{messages, reconstruct, tools};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlowAdmissionBackfillOutcome {
    pub(crate) examined: i64,
    pub(crate) admitted: i64,
}

impl PersistHandle {
    pub(crate) async fn backfill_set_predecessors(&self) -> Result<u64, PersistError> {
        let mut tx = self.background_pool().begin().await?;
        sqlx::query!("SET LOCAL statement_timeout = 0")
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query!(
            "WITH origins AS (
                 SELECT r.id, r.session_id, r.agent_instance_id, r.created_at,
                        pto.code AS turn_origin_code
                   FROM requests.requests r
                   JOIN requests.turn_origins pto ON pto.id = r.turn_origin_id
             ),
             flagged AS (
                 SELECT id, session_id, created_at,
                        COALESCE(agent_instance_id, 0) AS part_key,
                        ((agent_instance_id IS NULL AND turn_origin_code IN ('primary', 'primary_first'))
                      OR (agent_instance_id IS NOT NULL AND turn_origin_code = 'subagent')) AS is_candidate
                   FROM origins
             ),
             counted AS (
                 SELECT id, session_id, part_key, created_at, is_candidate,
                        SUM(CASE WHEN is_candidate THEN 1 ELSE 0 END) OVER (
                            PARTITION BY session_id, part_key ORDER BY created_at, id
                            ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
                        ) AS prior_candidate_count
                   FROM flagged
             ),
             candidates_ordinal AS (
                 SELECT id, session_id, part_key,
                        ROW_NUMBER() OVER (
                            PARTITION BY session_id, part_key ORDER BY created_at, id
                        ) AS ordinal
                   FROM flagged
                  WHERE is_candidate
             )
             UPDATE requests.requests r
                SET predecessor_request_id = co.id
               FROM counted c
               LEFT JOIN candidates_ordinal co
                      ON co.session_id = c.session_id
                     AND co.part_key   = c.part_key
                     AND co.ordinal    = c.prior_candidate_count
              WHERE c.id = r.id
                AND EXISTS (SELECT 1 FROM requests.messages m WHERE m.request_id = r.id)
                AND r.predecessor_request_id IS DISTINCT FROM co.id",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }
    pub(crate) async fn backfill_set_request_numbers(&self) -> Result<u64, PersistError> {
        let mut tx = self.background_pool().begin().await?;
        sqlx::query!("SET LOCAL statement_timeout = 0")
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query!(
            "UPDATE requests.requests r
                SET request_number = n.rn
               FROM (
                        SELECT id, ROW_NUMBER() OVER (
                                   PARTITION BY session_id ORDER BY created_at ASC, id ASC
                               ) AS rn
                          FROM requests.requests
                    ) n
              WHERE n.id = r.id
                AND r.request_number IS DISTINCT FROM n.rn",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }
    pub(crate) async fn find_multi_valued_delta_requests(&self) -> Result<Vec<i64>, PersistError> {
        let ids = sqlx::query_scalar!(
            r#"SELECT request_id AS "request_id!"
               FROM requests.deltas
              GROUP BY request_id
             HAVING COUNT(DISTINCT predecessor_request_id) > 1"#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }
    pub(crate) async fn backfill_recompute_deltas(
        &self,
        request_id: i64,
    ) -> Result<bool, PersistError> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query!(
            "SELECT session_id, predecessor_request_id
               FROM requests.requests
              WHERE id = $1",
            request_id,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(false);
        };

        let current = messages::fetch_canonical_bridge(&mut tx, request_id).await?;
        if current.is_empty() {
            tx.rollback().await?;
            return Ok(false);
        }

        let predecessor_bridge = match row.predecessor_request_id {
            Some(predecessor_request_id) => Some(
                messages::fetch_canonical_bridge(&mut tx, predecessor_request_id)
                    .await?
                    .into_iter()
                    .map(|m| (m.ord, m.sha))
                    .collect::<Vec<_>>(),
            ),
            None => None,
        };

        let delta_rows = messages::compute_delta(&current, predecessor_bridge.as_deref());
        messages::write_request_deltas(
            &mut tx,
            request_id,
            row.predecessor_request_id,
            row.session_id,
            &delta_rows,
        )
        .await?;

        tx.commit().await?;
        Ok(true)
    }
    pub(crate) async fn backfill_request_envelope_batch(
        &self,
        cursor: i64,
        to: Option<i64>,
        limit: i64,
    ) -> Result<Option<reconstruct::EnvelopeBatchOutcome>, PersistError> {
        reconstruct::backfill_envelope_batch(&self.pool, cursor, to, limit).await
    }
    pub(crate) async fn backfill_tool_definitions_candidates(
        &self,
        from: i64,
        to: Option<i64>,
        batch_size: i64,
    ) -> Result<Vec<i64>, PersistError> {
        let ids = match to {
            Some(to) => {
                sqlx::query_scalar!(
                    "SELECT DISTINCT rt.request_id
                       FROM requests.tools rt
                      WHERE rt.definition_id IS NULL
                        AND rt.request_id >= $1
                        AND rt.request_id <= $2
                      ORDER BY rt.request_id ASC
                      LIMIT $3",
                    from,
                    to,
                    batch_size,
                )
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_scalar!(
                    "SELECT DISTINCT rt.request_id
                       FROM requests.tools rt
                      WHERE rt.definition_id IS NULL
                        AND rt.request_id >= $1
                      ORDER BY rt.request_id ASC
                      LIMIT $2",
                    from,
                    batch_size,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(ids)
    }
    pub(crate) async fn backfill_apply_tool_definitions_for_request(
        &self,
        request_id: i64,
    ) -> Result<Option<usize>, PersistError> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query!(
            "SELECT request_body FROM requests.requests WHERE id = $1",
            request_id,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(body) = row.and_then(|r| r.request_body) else {
            tx.rollback().await?;
            return Ok(None);
        };

        let updated = tools::backfill_apply_definitions(&mut tx, request_id, &body).await?;
        tx.commit().await?;
        Ok(Some(updated))
    }
    pub(crate) async fn backfill_admit_complete_capture_sessions(
        &self,
    ) -> Result<FlowAdmissionBackfillOutcome, PersistError> {
        let mut tx = self.background_pool().begin().await?;
        sqlx::query!("SET LOCAL statement_timeout = 0")
            .execute(&mut *tx)
            .await?;
        let examined = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "examined!" FROM sessions.sessions WHERE flow_capture_version IS NULL"#,
        )
        .fetch_one(&mut *tx)
        .await?;
        let result = sqlx::query!(
            "WITH candidates AS (
                 SELECT id FROM sessions.sessions WHERE flow_capture_version IS NULL
             ),
             earliest AS (
                 SELECT DISTINCT ON (r.session_id)
                        r.session_id, r.message_count
                   FROM requests.requests r
                   JOIN candidates c ON c.id = r.session_id
                  ORDER BY r.session_id, r.created_at ASC, r.id ASC
             )
             UPDATE sessions.sessions s
                SET flow_capture_version = 1,
                    flow_admitted_at = clock_timestamp(),
                    flow_producer = $1,
                    flow_hook_event_name = $2,
                    flow_start_source = $3
               FROM earliest e
              WHERE e.session_id = s.id
                AND s.flow_capture_version IS NULL
                AND e.message_count = 1",
            flow_capture_admission::FLOW_PRODUCER,
            flow_capture_admission::FLOW_HOOK_EVENT_NAME,
            flow_capture_admission::FLOW_START_SOURCE_BACKFILL,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(FlowAdmissionBackfillOutcome {
            examined,
            admitted: result.rows_affected() as i64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    async fn null_out_flow_admission(pool: &sqlx::PgPool, session_id: &str) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE sessions.sessions
                SET flow_capture_version = NULL,
                    flow_admitted_at = NULL,
                    flow_producer = NULL,
                    flow_hook_event_name = NULL,
                    flow_start_source = NULL
              WHERE session_uuid = $1::text::uuid",
            session_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn flow_capture_version(
        pool: &sqlx::PgPool,
        session_id: &str,
    ) -> anyhow::Result<Option<i32>> {
        Ok(sqlx::query_scalar!(
            "SELECT flow_capture_version FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
            session_id,
        )
        .fetch_one(pool)
        .await?)
    }

    #[tokio::test]
    async fn backfill_admits_pre_existing_null_row_whose_earliest_request_is_message_one()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut request = crate::store::tests::make_test_request(None, &session_id);
        request.message_count = 1;
        handle
            .insert_request_initial(&request, true, IdMode::Mint)
            .await?;
        null_out_flow_admission(&handle.pool, &session_id).await?;
        assert_eq!(flow_capture_version(&handle.pool, &session_id).await?, None);

        let outcome = handle.backfill_admit_complete_capture_sessions().await?;
        assert_eq!(outcome.examined, 1);
        assert_eq!(outcome.admitted, 1);

        let admission = sqlx::query!(
            "SELECT flow_producer, flow_hook_event_name, flow_start_source
                 FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
            session_id,
        )
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            flow_capture_version(&handle.pool, &session_id).await?,
            Some(1)
        );
        assert_eq!(admission.flow_producer.as_deref(), Some("mike"));
        assert_eq!(admission.flow_hook_event_name.as_deref(), Some("capture"));
        assert_eq!(admission.flow_start_source.as_deref(), Some("backfill"));

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn backfill_leaves_partial_capture_session_returning_no_admitted_start()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut request = crate::store::tests::make_test_request(None, &session_id);
        request.message_count = 6;
        handle
            .insert_request_initial(&request, true, IdMode::Mint)
            .await?;
        assert_eq!(
            flow_capture_version(&handle.pool, &session_id).await?,
            None,
            "a first-sighted request with more than one message must never auto-admit"
        );

        let outcome = handle.backfill_admit_complete_capture_sessions().await?;
        assert_eq!(outcome.examined, 1);
        assert_eq!(
            outcome.admitted, 0,
            "a session mike joined mid-conversation must be left alone"
        );
        assert_eq!(
            flow_capture_version(&handle.pool, &session_id).await?,
            None,
            "must keep returning NoAdmittedStart — mike cannot prove capture is complete"
        );

        db.cleanup().await?;
        Ok(())
    }
}
