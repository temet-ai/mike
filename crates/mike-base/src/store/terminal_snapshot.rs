use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};

use super::RequestStore;
use super::models::{InterceptedRequest, ProxyEvent, RequestId};
use super::operator_turns;
use super::persistence::{IdMode, PersistHandle, RequestInsertOutcome, RowInsert};
use crate::analysis::EmbeddingService;

struct SnapshotContext {
    persist: PersistHandle,
    req: InterceptedRequest,
    embedding: Option<(EmbeddingService, Arc<tokio::sync::Semaphore>)>,
    turn_close_event_tx: Option<broadcast::Sender<ProxyEvent>>,
}

impl RequestStore {
    pub async fn snapshot_terminal(
        store: &Arc<RwLock<RequestStore>>,
        request_id: RequestId,
    ) {
        if Self::hand_off_deferred(store, request_id).await {
            return;
        }
        let Some(ctx) = Self::load_snapshot_context(store, request_id).await else {
            return;
        };

        if !Self::ensure_stub_row(&ctx, request_id).await {
            return;
        }

        let Some(closed_turn_id) =
            Self::write_snapshot_and_remove_entry(store, &ctx, request_id).await
        else {
            return;
        };

        Self::backfill_agent_instances(&ctx, request_id).await;

        if let Some(turn_id) = closed_turn_id {
            Self::spawn_embed_for_closed_turn(&ctx, turn_id);
            Self::notify_turn_closed(&ctx, request_id, turn_id);
        }
    }

    async fn load_snapshot_context(
        store: &Arc<RwLock<RequestStore>>,
        request_id: RequestId,
    ) -> Option<SnapshotContext> {
        let s = store.read().await;
        let persist = s.persist().cloned()?;
        let req = s.get(&request_id).cloned()?;
        if !req.status.is_terminal() {
            return None;
        }
        let embedding = s.embedding().map(|(svc, sem)| (svc.clone(), sem.clone()));
        let turn_close_event_tx = s.analysis_wired().map(|(_, tx)| tx.clone());
        Some(SnapshotContext {
            persist,
            req,
            embedding,
            turn_close_event_tx,
        })
    }

    async fn ensure_stub_row(ctx: &SnapshotContext, request_id: RequestId) -> bool {
        match ctx
            .persist
            .insert_request_initial(&ctx.req, true, IdMode::Ensure)
            .await
        {
            Err(e) => {
                Self::warn_ensure_stub_row_failed(ctx, request_id, &e);
                false
            }
            Ok(RequestInsertOutcome {
                row: RowInsert::AlreadyExisted,
                ..
            }) => true,
            Ok(RequestInsertOutcome {
                row: RowInsert::Inserted,
                ..
            }) => {
                Self::warn_stub_absent_at_terminal(ctx, request_id);
                true
            }
        }
    }

    fn warn_ensure_stub_row_failed(
        ctx: &SnapshotContext,
        request_id: RequestId,
        error: &crate::store::persistence::PersistError,
    ) {
        tracing::warn!(
            request_id = %request_id,
            session_id = %ctx.req.session_id,
            error = %error,
            "failed to ensure stub row before snapshot — skipping terminal write"
        );
    }

    fn warn_stub_absent_at_terminal(ctx: &SnapshotContext, request_id: RequestId) {
        tracing::warn!(
            request_id = %request_id,
            session_id = %ctx.req.session_id,
            "phase-1 stub absent at terminal — recovered"
        );
    }

    async fn write_snapshot_and_remove_entry(
        store: &Arc<RwLock<RequestStore>>,
        ctx: &SnapshotContext,
        request_id: RequestId,
    ) -> Option<Option<i64>> {
        match ctx.persist.snapshot_request(&ctx.req).await {
            Err(e) => {
                tracing::warn!(
                    request_id = %request_id,
                    session_id = %ctx.req.session_id,
                    error = %e,
                    "failed to snapshot terminal request — proxy continues, retry on next terminal event"
                );
                None
            }
            Ok((title_opt, closed)) => {
                {
                    let mut s = store.write().await;
                    if let Some(title) = title_opt {
                        s.set_session_title(&ctx.req.session_id, title);
                    }
                    s.remove_entry(&request_id);
                }
                Some(closed)
            }
        }
    }

    async fn backfill_agent_instances(ctx: &SnapshotContext, request_id: RequestId) {
        if (ctx.req.turn_origin == "primary" || ctx.req.turn_origin == "primary_first")
            && let Err(e) = ctx
                .persist
                .create_instances_for_dispatches(&ctx.req.session_id)
                .await
        {
            tracing::warn!(
                request_id = %request_id,
                session_id = %ctx.req.session_id,
                error = %e,
                "create_instances_for_dispatches failed — agent attribution skipped"
            );
        }
    }

    fn spawn_embed_for_closed_turn(ctx: &SnapshotContext, turn_id: i64) {
        let Some((service, permits)) = ctx.embedding.clone() else {
            return;
        };
        if !service.is_available() {
            return;
        }
        match permits.try_acquire_owned() {
            Ok(permit) => {
                let persist_for_embed = ctx.persist.clone();
                crate::observability::spans::spawn_detached(async move {
                    let _permit = permit;
                    if let Err(e) =
                        operator_turns::embed_turn(&persist_for_embed, &service, turn_id).await
                    {
                        tracing::warn!(
                            turn_id,
                            error = %e,
                            "best-effort turn embedding failed; backfill will pick up"
                        );
                    }
                });
            }
            Err(_) => {
                tracing::debug!(
                    session_id = %ctx.req.session_id,
                    turn_id,
                    "embedding semaphore saturated — turn left for periodic backfill"
                );
            }
        }
    }

    fn notify_turn_closed(ctx: &SnapshotContext, request_id: RequestId, turn_id: i64) {
        let Some(tx) = ctx.turn_close_event_tx.as_ref() else {
            return;
        };
        let _ = tx.send(ProxyEvent::OperatorTurnClosed {
            closed_turn_id: turn_id,
            session_id: ctx.req.session_id.clone(),
            new_request_id: request_id,
            human_prompt: ctx.req.human_prompt.clone(),
        });
    }
}
