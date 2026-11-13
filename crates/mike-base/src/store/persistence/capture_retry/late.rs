use std::time::{Duration, Instant};

use super::{WriteFailure, classify, ledger};
use crate::store::models::{InterceptedRequest, RequestId, is_fallback};
use crate::store::persistence::{IdMode, PersistError, PersistHandle};

const FIRST_DELAY: Duration = Duration::from_secs(1);
const MAX_DELAY: Duration = Duration::from_secs(60);
const GIVE_UP_AFTER: Duration = Duration::from_secs(30 * 60);

pub async fn record_late(persist: PersistHandle, mut req: InterceptedRequest) -> bool {
    let key = req.id;
    match write_once(&persist, &mut req).await {
        Ok(db_id) => {
            finish(key, db_id, &req);
            true
        }
        Err(error) => {
            match classify(&error) {
                WriteFailure::Temporary(reason) => {
                    tracing::warn!(
                        session_id = %req.session_id,
                        reason,
                        "late write of a request whose first write failed did not land yet; retrying"
                    );
                    crate::observability::spans::spawn_detached(retry_until_recorded(
                        persist, key, req,
                    ));
                }
                other => ledger::note_gave_up(key, other.reason()),
            }
            false
        }
    }
}

async fn write_once(
    persist: &PersistHandle,
    req: &mut InterceptedRequest,
) -> Result<i64, PersistError> {
    if is_fallback(req.id.as_u64()) {
        let outcome = persist
            .insert_request_initial(req, false, IdMode::Mint)
            .await?;
        req.id = RequestId::from_raw(outcome.id as u64);
    }
    persist.snapshot_request(req).await?;
    Ok(req.id.as_u64() as i64)
}

async fn retry_until_recorded(persist: PersistHandle, key: RequestId, mut req: InterceptedRequest) {
    let started = Instant::now();
    let mut delay = FIRST_DELAY;
    loop {
        tokio::time::sleep(delay).await;
        let error = match write_once(&persist, &mut req).await {
            Ok(db_id) => return finish(key, db_id, &req),
            Err(error) => error,
        };
        let failure = classify(&error);
        if !matches!(failure, WriteFailure::Temporary(_)) || started.elapsed() >= GIVE_UP_AFTER {
            return ledger::note_gave_up(key, failure.reason());
        }
        delay = (delay * 2).min(MAX_DELAY);
    }
}

fn finish(key: RequestId, db_id: i64, req: &InterceptedRequest) {
    ledger::note_recorded_late(key);
    tracing::info!(
        request_id = db_id,
        session_id = %req.session_id,
        request_at = %req.created_at,
        "recorded a request whose first write had failed"
    );
}
