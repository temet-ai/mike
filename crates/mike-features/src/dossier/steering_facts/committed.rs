use crate::launch::open_changes::open_change_paths;
use crate::store::persistence::PersistHandle;

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
    let Some(open) = open_change_paths(persist, &checkout).await else {
        return;
    };
    mark_committed(&mut observation.changed, &checkout, &open);
}
