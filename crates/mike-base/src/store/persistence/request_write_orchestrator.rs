use super::request_write::{
    IdMode, RequestEnvelopePrep, RequestInsertOutcome, RowInsert, fallback_id_skip_outcome,
    insert_requests_row, resolve_capture_session_and_project,
};
use super::request_write_tool_io::InsertedToolResult;
use super::{PersistError, PersistHandle};
use crate::store::models::InterceptedRequest;

impl PersistHandle {
    pub async fn insert_request_initial(
        &self,
        req: &InterceptedRequest,
        is_new_session: bool,
        id_mode: IdMode,
    ) -> Result<RequestInsertOutcome, PersistError> {
        crate::metrics::time_db("insert_request_initial", async move {
            if let Some(early) = fallback_id_skip_outcome(id_mode, req) {
                return Ok(early);
            }
            let req = super::text_sanitize::request_with_clean_text(req);
            let req = req.as_ref();
            let prep = RequestEnvelopePrep::compute(req)?;
            let session_epoch = self.session_epoch();
            let mut tx = self.pool.begin().await?;

            let (session_db_id, project_id) =
                resolve_capture_session_and_project(&mut tx, req, is_new_session).await?;
            let returned_id =
                insert_requests_row(&mut tx, req, id_mode, session_db_id, project_id, &prep)
                    .await?;
            let row_inserted = returned_id.is_some();
            let request_db_id = returned_id.unwrap_or(prep.request_id);

            let inserted_results = self
                .write_dependent_rows_or_keep_request(&mut tx, req, session_db_id, request_db_id)
                .await?;

            if row_inserted {
                sqlx::query!(
                    "UPDATE sessions.sessions
                     SET last_seen   = GREATEST(last_seen, $2),
                         last_status = 'Pending'
                     WHERE id = $1",
                    session_db_id,
                    req.created_at,
                )
                .execute(&mut *tx)
                .await?;
            }

            tx.commit().await?;
            self.remember_committed_session(session_epoch, &req.session_id, session_db_id);

            let dispatch_wire_ids = if row_inserted {
                super::request_write_worker_producer::dispatch_result_wire_ids(
                    &req.tool_results_in_body,
                    &inserted_results,
                )
            } else {
                Vec::new()
            };
            self.reconcile_after_commit(request_db_id, &dispatch_wire_ids)
                .await;

            if is_new_session
                && let Ok(session_uuid) = req.session_id.parse::<uuid::Uuid>()
                && let Some(hook) = self.new_session_hook()
                && let Err(e) = hook.on_new_session(self, session_uuid).await
            {
                tracing::warn!(
                    session_id = %req.session_id,
                    error = %e,
                    "insert_request_initial: auto-enrol failed (best-effort)"
                );
            }

            if row_inserted {
                tracing::debug!(
                    request_id = request_db_id,
                    session_id = %req.session_id,
                    project_id,
                    "persisted initial request stub (status=Pending)"
                );
                return Ok(RequestInsertOutcome {
                    id: request_db_id,
                    row: RowInsert::Inserted,
                    session_id: Some(session_db_id),
                });
            }

            Ok(RequestInsertOutcome {
                id: request_db_id,
                row: RowInsert::AlreadyExisted,
                session_id: Some(session_db_id),
            })
        })
        .await
    }

    async fn write_dependent_rows_or_keep_request(
        &self,
        tx: &mut sqlx::PgConnection,
        req: &InterceptedRequest,
        session_db_id: i64,
        request_db_id: i64,
    ) -> Result<Vec<InsertedToolResult>, PersistError> {
        let mut savepoint = sqlx::Acquire::begin(&mut *tx).await?;
        let written = self
            .write_request_dependent_rows(&mut savepoint, req, session_db_id, request_db_id)
            .await;
        let error = match written {
            Ok(results) => {
                savepoint.commit().await?;
                return Ok(results);
            }
            Err(error) => error,
        };
        let super::capture_retry::WriteFailure::Refused(reason) =
            super::capture_retry::classify(&error)
        else {
            return Err(error);
        };
        savepoint.rollback().await?;
        tracing::error!(
            request_id = request_db_id,
            session_id = %req.session_id,
            reason,
            "insert_request_initial: rows derived from the body were refused; \
             the request row and its raw body are kept"
        );
        Ok(Vec::new())
    }

    async fn write_request_dependent_rows(
        &self,
        tx: &mut sqlx::PgConnection,
        req: &InterceptedRequest,
        session_db_id: i64,
        request_db_id: i64,
    ) -> Result<Vec<InsertedToolResult>, PersistError> {
        super::request_write_aux::write_system_prompt_and_firing_rules(
            &mut *tx,
            req,
            request_db_id,
        )
        .await?;
        super::request_write_aux::write_declared_tools_bridge(
            &mut *tx,
            req,
            session_db_id,
            request_db_id,
        )
        .await?;
        super::request_write_aux::attribute_inline_agent_instance(
            &mut *tx,
            req,
            session_db_id,
            request_db_id,
        )
        .await?;
        super::request_write_message_bridge::write_message_dedup_bridge(
            &mut *tx,
            req,
            session_db_id,
            request_db_id,
        )
        .await?;
        super::request_write_tool_io::write_tool_uses_and_dispatches(
            &mut *tx,
            req,
            request_db_id,
            session_db_id,
        )
        .await?;
        super::request_write_tool_io::retry_failed_stream_blocks(
            &mut *tx,
            req,
            request_db_id,
            session_db_id,
        )
        .await?;
        let inserted_results = super::request_write_tool_io::write_tool_results(
            &mut *tx,
            req,
            request_db_id,
            session_db_id,
        )
        .await?;
        super::work_items::record_from_request_guarded(&mut *tx, req, session_db_id, request_db_id)
            .await?;
        super::request_write_worker_producer::write_worker_producer_ids(&mut *tx, req).await?;
        self.capture_session_prompt_lane(&mut *tx, req, request_db_id, session_db_id)
            .await?;
        Ok(inserted_results)
    }

    async fn reconcile_after_commit(&self, request_db_id: i64, wire_ids: &[String]) {
        let result: Result<(), PersistError> = async {
            let mut tx = self.pool.begin().await?;
            if !wire_ids.is_empty() {
                super::turn_lifecycle::reconcile_dispatch_results(&mut tx, wire_ids).await?;
            }
            super::turn_lifecycle::attribute_inherited_requests(&mut tx, request_db_id).await?;
            tx.commit().await?;
            Ok(())
        }
        .await;
        if let Err(e) = result {
            tracing::warn!(
                request_id = request_db_id,
                error = %e,
                "insert_request_initial: turn-attribution reconciliation failed (best-effort)"
            );
        }
    }
}
