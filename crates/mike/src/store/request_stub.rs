use std::sync::Arc;

use tokio::sync::RwLock;

use super::RequestStore;
use super::models::{InterceptedRequest, RequestId};
use super::persistence::{self, IdMode, PersistHandle};

impl RequestStore {
    pub(crate) async fn insert_request_stub_to_db(
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
                    return Self::warn_stub_insert_failed(&req.session_id, &e);
                }
            }
        }
    }

    fn warn_stub_insert_failed(session_id: &str, error: &persistence::PersistError) -> RequestId {
        let fallback = RequestId::fallback();
        tracing::warn!(
            request_id = %fallback,
            session_id = %session_id,
            error = %error,
            "insert_request_stub_to_db failed — minting non-persisted fallback id; chunk FK writes will fail; terminal snapshot skips persistence"
        );
        fallback
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

    pub(crate) async fn close_aborted_request_in_db(
        store: &Arc<RwLock<RequestStore>>,
        request_id: RequestId,
    ) {
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
