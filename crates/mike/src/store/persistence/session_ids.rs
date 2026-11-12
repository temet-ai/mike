use std::future::Future;

use uuid::Uuid;

use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn session_row_id(
        &self,
        session_uuid: Uuid,
    ) -> Result<Option<i64>, PersistError> {
        self.session_row_id_in(&self.pool, session_uuid).await
    }

    pub(super) async fn session_row_id_in<'e, E>(
        &self,
        executor: E,
        session_uuid: Uuid,
    ) -> Result<Option<i64>, PersistError>
    where
        E: sqlx::PgExecutor<'e>,
    {
        if let Some(id) = self.id_cache.session(&session_uuid) {
            return Ok(Some(id));
        }
        let epoch = self.id_cache.session_epoch();
        let id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
                .bind(session_uuid)
                .fetch_optional(executor)
                .await?;
        if let Some(id) = id {
            self.id_cache.remember_session(epoch, session_uuid, id);
        }
        Ok(id)
    }

    pub(super) async fn cached_session_or_ensure<F>(
        &self,
        session_id: &str,
        ensure: F,
    ) -> Result<i64, PersistError>
    where
        F: Future<Output = Result<i64, PersistError>>,
    {
        let session_uuid = session_id.parse::<Uuid>().ok();
        if let Some(id) = session_uuid.and_then(|uuid| self.id_cache.session(&uuid)) {
            return Ok(id);
        }
        let epoch = self.id_cache.session_epoch();
        let id = ensure.await?;
        if let Some(uuid) = session_uuid {
            self.id_cache.remember_session(epoch, uuid, id);
        }
        Ok(id)
    }

    pub(super) fn session_epoch(&self) -> u64 {
        self.id_cache.session_epoch()
    }

    pub(super) fn remember_committed_session(&self, read_at_epoch: u64, session_id: &str, id: i64) {
        if let Ok(session_uuid) = session_id.parse::<Uuid>() {
            self.id_cache
                .remember_session(read_at_epoch, session_uuid, id);
        }
    }

    pub(super) fn forget_cached_sessions(&self) {
        self.id_cache.forget_sessions();
    }

    pub(super) fn forget_cached_projects(&self) {
        self.id_cache.forget_projects();
    }
}

impl PersistHandle {
    pub(in crate::store::persistence) fn session_row_known(&self, session: uuid::Uuid) -> bool {
        self.id_cache.session(&session).is_some()
    }
}
