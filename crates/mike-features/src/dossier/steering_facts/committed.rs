use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

use super::changed::mark_committed;
use super::observation::Observation;

pub async fn mark_session_changes(
    persist: &PersistHandle,
    session_uuid: Option<uuid::Uuid>,
    observation: &mut Observation,
) {
    if observation.changed.is_empty() {
        return;
    }
    let Some(uuid) = session_uuid else {
        return;
    };
    let Ok(Some(identity)) = persist.session_identity_by_uuid(uuid).await else {
        return;
    };
    let Some(checkout) = identity.cwd else {
        return;
    };
    if !matches!(
        persist.fleet().newest_change_event(&checkout).await,
        Ok(Some(_))
    ) {
        return;
    }
    let open = match persist.fleet().open_changes(&checkout).await {
        Ok(rows) => rows.into_iter().map(|r| r.path).collect::<Vec<_>>(),
        Err(error) => {
            tracing::debug!(%error, "steering facts: open changes unavailable; committed left unknown");
            return;
        }
    };
    mark_committed(&mut observation.changed, &checkout, &open);
}
