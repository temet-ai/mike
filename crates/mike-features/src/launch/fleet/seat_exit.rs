use crate::launch::local_root::{self, RootKey};
use crate::launch::{LaunchError, backend_for};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::UnsettledFleetLaunch;
use crate::store::persistence::proxy_repo::ProxyPersist as _;
use crate::workspace::Liveness;

pub fn key(run: &str, seat: &UnsettledFleetLaunch) -> Option<RootKey> {
    RootKey::from_record(
        seat.id,
        run,
        &seat.backend,
        seat.session_uuid,
        seat.surface_ref.as_deref(),
        seat.pid,
    )
}

pub async fn observe(
    persist: &PersistHandle,
    run: &str,
    seat: &UnsettledFleetLaunch,
) -> Result<Option<bool>, LaunchError> {
    if let Some(exited) = observe_root(run, seat).await? {
        return Ok(Some(exited));
    }
    Ok(session_ended(persist, seat).await.then_some(true))
}

async fn session_ended(persist: &PersistHandle, seat: &UnsettledFleetLaunch) -> bool {
    let Some(session) = seat.session_uuid.filter(|_| seat.opened) else {
        return false;
    };
    matches!(
        persist.proxy().fetch_session_liveness(session).await,
        Ok(Some(Liveness::Ended))
    )
}

async fn observe_root(run: &str, seat: &UnsettledFleetLaunch) -> Result<Option<bool>, LaunchError> {
    let Some(key) = key(run, seat) else {
        return Ok(None);
    };
    if let Some(exited) = local_root::observed_exit(&key) {
        return Ok(Some(exited));
    }
    let Some(backend) = backend_for(seat.backend.parse()?) else {
        return Ok(None);
    };
    let Some(session) = seat.session_uuid else {
        return Ok(None);
    };
    if let Some(surface) = backend.find_by_session(session).await? {
        local_root::bind_current(&key, &surface);
    }
    Ok(local_root::observed_exit(&key))
}

pub fn forget(run: &str, seat: &UnsettledFleetLaunch) {
    if let Some(key) = key(run, seat) {
        local_root::forget(&key);
    }
}

#[cfg(test)]
#[path = "seat_exit_tests.rs"]
mod tests;
