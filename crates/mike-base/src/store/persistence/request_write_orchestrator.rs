use super::request_write::{
    IdMode, RequestEnvelopePrep, RequestInsertOutcome, RowInsert, fallback_id_skip_outcome,
    insert_requests_row, resolve_capture_session_and_project,
};
use super::request_write_tool_io::InsertedToolResult;
use super::{PersistError, PersistHandle};
use crate::store::models::InterceptedRequest;

struct DependentRows {
    tool_results: Vec<InsertedToolResult>,
    agent_instance_id: Option<i64>,
}

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
            let is_new_session = is_new_session && !self.session_set_up(&req.session_id);
            let prep = RequestEnvelopePrep::compute(req)?;
            let session_epoch = self.session_epoch();
            let mut tx = self.pool.begin().await?;

            let (session_db_id, project_id) =
                resolve_capture_session_and_project(&mut tx, req, is_new_session).await?;
            if is_new_session && let Some(machine) = self.host_machine() {
                super::session_machine::stamp_session_machine_by_id(
                    &mut tx,
                    session_db_id,
                    machine,
                )
                .await?;
            }
            let returned_id =
                insert_requests_row(&mut tx, req, id_mode, session_db_id, project_id, &prep)
                    .await?;
            let row_inserted = returned_id.is_some();
            let request_db_id = returned_id.unwrap_or(prep.request_id);

            let DependentRows {
                tool_results: inserted_results,
                agent_instance_id,
            } = self
                .write_dependent_rows_or_keep_request(
                    &mut tx,
                    req,
                    (session_db_id, request_db_id),
                    row_inserted,
                )
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
            if row_inserted {
                self.note_capture_write(req.created_at);
            }

            let dispatch_wire_ids = if row_inserted {
                super::request_write_worker_producer::dispatch_result_wire_ids(
                    &req.tool_results_in_body,
                    &inserted_results,
                )
            } else {
                Vec::new()
            };
            self.reconcile_after_commit(
                request_db_id,
                &dispatch_wire_ids,
                req.turn_origin == "subagent",
            )
            .await;

            if is_new_session {
                self.set_up_new_session(&req.session_id).await;
            }

            if row_inserted {
                tracing::debug!(
                    request_id = request_db_id,
                    session_id = %req.session_id,
                    project_id,
                    "persisted initial request stub (status=Pending)"
                );
            }

            Ok(RequestInsertOutcome {
                id: request_db_id,
                row: if row_inserted {
                    RowInsert::Inserted
                } else {
                    RowInsert::AlreadyExisted
                },
                session_id: Some(session_db_id),
                agent_instance_id,
            })
        })
        .await
    }

    /// The terminal write for a request whose stub row committed: repeats only
    /// the live writes that did not land (tool uses, stream blocks) and the
    /// subagent turn attribution, instead of the whole initial insert.
    pub async fn write_terminal_backstop(
        &self,
        req: &InterceptedRequest,
    ) -> Result<(), PersistError> {
        let tool_uses =
            req.capture_writes.tool_use_failed || req.tool_calls.iter().any(|call| !call.complete);
        let subagent = req.turn_origin == "subagent";
        if !tool_uses && !subagent && req.failed_stream_blocks.is_empty() {
            return Ok(());
        }
        crate::metrics::time_db("write_terminal_backstop", async move {
            let request_db_id = req.id.as_u64() as i64;
            if tool_uses || !req.failed_stream_blocks.is_empty() {
                let Ok(session_uuid) = req.session_id.parse::<uuid::Uuid>() else {
                    return Ok(());
                };
                let Some(session_db_id) = self.session_row_id(session_uuid).await? else {
                    return Ok(());
                };
                let req = super::text_sanitize::request_with_clean_text(req);
                let req = req.as_ref();
                let mut tx = self.pool.begin().await?;
                if tool_uses {
                    super::request_write_tool_io::write_tool_uses_and_dispatches(
                        &mut tx,
                        req,
                        request_db_id,
                        session_db_id,
                    )
                    .await?;
                }
                super::request_write_tool_io::retry_failed_stream_blocks(
                    &mut tx,
                    req,
                    request_db_id,
                    session_db_id,
                )
                .await?;
                tx.commit().await?;
            }
            if subagent {
                self.reconcile_after_commit(request_db_id, &[], true).await;
            }
            Ok(())
        })
        .await
    }

    async fn set_up_new_session(&self, session_id: &str) {
        self.note_session_arrived();
        let hooked = match (session_id.parse::<uuid::Uuid>(), self.new_session_hook()) {
            (Ok(session_uuid), Some(hook)) => hook.on_new_session(self, session_uuid).await,
            _ => Ok(()),
        };
        match hooked {
            Ok(()) => self.mark_session_set_up(session_id),
            Err(e) => tracing::warn!(
                session_id = %session_id,
                error = %e,
                "insert_request_initial: auto-enrol failed (best-effort)"
            ),
        }
    }

    async fn write_dependent_rows_or_keep_request(
        &self,
        tx: &mut sqlx::PgConnection,
        req: &InterceptedRequest,
        (session_db_id, request_db_id): (i64, i64),
        fresh_row: bool,
    ) -> Result<DependentRows, PersistError> {
        let mut savepoint = sqlx::Acquire::begin(&mut *tx).await?;
        let written = self
            .write_request_dependent_rows(
                &mut savepoint,
                req,
                (session_db_id, request_db_id),
                fresh_row,
            )
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
        Ok(DependentRows {
            tool_results: Vec::new(),
            agent_instance_id: req.agent_instance_id,
        })
    }

    async fn write_request_dependent_rows(
        &self,
        tx: &mut sqlx::PgConnection,
        req: &InterceptedRequest,
        (session_db_id, request_db_id): (i64, i64),
        fresh_row: bool,
    ) -> Result<DependentRows, PersistError> {
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
        let agent_instance_id = super::agent_instance_capture::attribute_inline_agent_instance(
            &mut *tx,
            req,
            session_db_id,
            request_db_id,
        )
        .await?;
        super::request_write_message_bridge::write_message_dedup_bridge(
            &mut *tx,
            req,
            agent_instance_id,
            session_db_id,
            request_db_id,
            fresh_row,
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
        Ok(DependentRows {
            tool_results: inserted_results,
            agent_instance_id,
        })
    }

    /// Links dispatch results to their parents and, for a subagent request,
    /// stamps the operator turn it inherits. Turn inheritance only ever
    /// matches a request whose origin is `subagent`, so others skip it.
    async fn reconcile_after_commit(
        &self,
        request_db_id: i64,
        wire_ids: &[String],
        subagent: bool,
    ) {
        if wire_ids.is_empty() && !subagent {
            return;
        }
        let result: Result<(), PersistError> = async {
            let mut tx = self.pool.begin().await?;
            if !wire_ids.is_empty() {
                super::turn_lifecycle::reconcile_dispatch_results(&mut tx, wire_ids).await?;
            }
            if subagent {
                super::turn_lifecycle::attribute_inherited_requests(&mut tx, request_db_id).await?;
            }
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
