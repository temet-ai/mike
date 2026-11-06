use crate::launch::local_root::{self, RootKey};
use crate::launch::{LaunchError, backend_for};
use crate::store::persistence::fleet::UnsettledFleetLaunch;

pub(super) fn key(run: &str, seat: &UnsettledFleetLaunch) -> Option<RootKey> {
    RootKey::from_record(
        seat.id,
        run,
        &seat.backend,
        seat.session_uuid,
        seat.surface_ref.as_deref(),
        seat.pid,
    )
}

pub(in crate::launch::fleet) async fn observe(
    run: &str,
    seat: &UnsettledFleetLaunch,
) -> Result<Option<bool>, LaunchError> {
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

pub(super) fn forget(run: &str, seat: &UnsettledFleetLaunch) {
    if let Some(key) = key(run, seat) {
        local_root::forget(&key);
    }
}
