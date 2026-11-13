use std::sync::Arc;

use tokio::sync::RwLock;

use super::RequestStore;
use super::models::{InterceptedRequest, RequestId};
use super::persistence::{self, IdMode, PersistHandle, capture_retry};

impl RequestStore {
    pub async fn insert_request_stub_to_db(
        store: &Arc<RwLock<RequestStore>>,
        req: &InterceptedRequest,
        is_new_session: bool,
    ) -> RequestId {
        let persist = {
            let s = store.read().await;
            match s.persist().cloned() {
                Some(handle) => handle,
                None => return req.id,
            }
        };
        Self::insert_stub_with_retry(store, &persist, req, is_new_session).await
    }

    async fn insert_stub_with_retry(
        store: &Arc<RwLock<RequestStore>>,
        persist: &PersistHandle,
        req: &InterceptedRequest,
        is_new_session: bool,
    ) -> RequestId {
        const MAX_ATTEMPTS: u8 = 3;
        let mut attempt = 0u8;
        loop {
            match persist
                .insert_request_initial(req, is_new_session, IdMode::Mint)
                .await
            {
                Ok(outcome) => {
                    Self::stamp_session_db_id(store, &req.session_id, outcome.session_id).await;
                    return RequestId::from_raw(outcome.id as u64);
                }
                Err(persistence::PersistError::Database(sqlx::Error::RowNotFound))
                    if attempt + 1 < MAX_ATTEMPTS =>
                {
                    attempt += 1;
                    tracing::debug!(
                        session_id = %req.session_id,
                        attempt,
                        "insert_request_stub_to_db: session/project bootstrap race, retrying"
                    );
                }
                Err(e) => {
                    return Self::stub_insert_failed(req, &e);
                }
            }
        }
    }

    fn stub_insert_failed(
        req: &InterceptedRequest,
        error: &persistence::PersistError,
    ) -> RequestId {
        let fallback = RequestId::fallback();
        match capture_retry::classify(error) {
            capture_retry::WriteFailure::Temporary(reason) => {
                capture_retry::defer(fallback, req, &reason);
                Self::warn_deferred(fallback, req, &reason);
            }
            other => {
                Self::warn_refused(fallback, req, error);
                capture_retry::note_unrecorded(&req.session_id, req.created_at, other.reason());
            }
        }
        fallback
    }

    fn warn_deferred(fallback: RequestId, req: &InterceptedRequest, reason: &str) {
        tracing::warn!(
            request_id = %fallback,
            session_id = %req.session_id,
            reason,
            "first write of the request failed on a temporary database error; \
             it is kept and written when its answer completes"
        );
    }

    fn warn_refused(
        fallback: RequestId,
        req: &InterceptedRequest,
        error: &persistence::PersistError,
    ) {
        tracing::warn!(
            request_id = %fallback,
            session_id = %req.session_id,
            error = %error,
            "first write of the request was refused by the database; it is not recorded"
        );
    }

    pub async fn hand_off_deferred(
        store: &Arc<RwLock<RequestStore>>,
        request_id: RequestId,
    ) -> bool {
        if !super::models::is_fallback(request_id.as_u64())
            || !capture_retry::is_deferred(request_id)
        {
            return false;
        }
        let taken = {
            let mut s = store.write().await;
            let persist = s.persist().cloned();
            let req = s
                .get(&request_id)
                .filter(|r| r.status.is_terminal())
                .cloned();
            if req.is_some() {
                s.remove_entry(&request_id);
            }
            persist.zip(req)
        };
        if let Some((persist, req)) = taken {
            capture_retry::record_late(persist, req).await;
        }
        true
    }

    async fn stamp_session_db_id(
        store: &Arc<RwLock<RequestStore>>,
        session_id: &str,
        session_db_id: Option<i64>,
    ) {
        let Some(db_id) = session_db_id else {
            return;
        };
        let mut s = store.write().await;
        s.session_meta
            .entry(session_id.to_string())
            .or_default()
            .session_id = Some(db_id);
    }

    pub async fn close_aborted_request_in_db(
        store: &Arc<RwLock<RequestStore>>,
        request_id: RequestId,
    ) {
        if Self::hand_off_deferred(store, request_id).await {
            return;
        }
        let persist = {
            let s = store.read().await;
            let Some(handle) = s.persist().cloned() else {
                return;
            };
            if s.get(&request_id).is_none() {
                return;
            };
            handle
        };

        if let Err(e) = persist.close_aborted_request(request_id.as_u64()).await {
            tracing::warn!(
                request_id = %request_id,
                error = %e,
                "close_aborted_request_in_db failed — Pending row may linger until crash-recovery sweep"
            );
        }
    }
}
