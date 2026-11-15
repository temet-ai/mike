use crate::store::persistence::dossier_repo::DossierPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::future::Future;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) const QUIET_LIMIT: Duration = Duration::hours(6);

pub(crate) trait FixerSessions: Send + Sync {
    fn ended(
        &self,
        session: Uuid,
        handed_off_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<String>, PersistError>> + Send;
}

pub(crate) struct LedgerSessions {
    pub(crate) persist: PersistHandle,
}

pub(crate) fn silent_for(last: DateTime<Utc>, now: DateTime<Utc>, session: Uuid) -> Option<String> {
    (now - last >= QUIET_LIMIT).then(|| {
        format!(
            "fixer session {session} has made no request since {}",
            last.to_rfc3339()
        )
    })
}

impl LedgerSessions {
    async fn last_request(&self, session: Uuid) -> Result<Option<DateTime<Utc>>, PersistError> {
        let ids = self
            .persist
            .dossier()
            .session_db_ids_by_uuid(&[session])
            .await?;
        match ids.get(&session) {
            Some(id) => self.persist.latest_request_at(*id).await,
            None => Ok(None),
        }
    }
}

impl FixerSessions for LedgerSessions {
    async fn ended(
        &self,
        session: Uuid,
        handed_off_at: DateTime<Utc>,
    ) -> Result<Option<String>, PersistError> {
        let launch = self
            .persist
            .launch()
            .find_launch_by_session(session)
            .await?;
        if let Some(closed) = launch.as_ref().and_then(|row| row.closed_at) {
            let why = launch
                .as_ref()
                .and_then(|row| row.close_reason.clone())
                .unwrap_or_else(|| "no reason recorded".to_owned());
            return Ok(Some(format!(
                "fixer session {session}'s launch closed at {}: {why}",
                closed.to_rfc3339()
            )));
        }
        let last = self
            .last_request(session)
            .await?
            .map_or(handed_off_at, |seen| seen.max(handed_off_at));
        Ok(silent_for(last, Utc::now(), session))
    }
}
