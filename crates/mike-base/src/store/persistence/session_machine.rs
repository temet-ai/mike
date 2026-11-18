use std::sync::OnceLock;

use serde::Serialize;
use uuid::Uuid;

use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MachineRef {
    pub uuid: Uuid,
    pub name: Option<String>,
}

impl MachineRef {
    pub fn from_parts(uuid: Option<Uuid>, name: Option<String>) -> Option<Self> {
        uuid.map(|uuid| Self { uuid, name })
    }
}

#[derive(Default)]
struct HostMachineSlot(OnceLock<Uuid>);

impl PersistHandle {
    pub fn set_host_machine(&self, machine: Uuid) {
        if self.memo::<HostMachineSlot>().0.set(machine).is_err() {
            tracing::debug!("the host machine was already set on this handle");
        }
    }

    pub fn host_machine(&self) -> Option<Uuid> {
        self.memo::<HostMachineSlot>().0.get().copied()
    }
}

pub(super) async fn stamp_session_machine_by_id(
    tx: &mut sqlx::PgConnection,
    session_db_id: i64,
    machine: Uuid,
) -> Result<(), PersistError> {
    sqlx::query!(
        "UPDATE sessions.sessions SET machine_uuid = $2
          WHERE id = $1 AND machine_uuid IS NULL",
        session_db_id,
        machine,
    )
    .execute(tx)
    .await?;
    Ok(())
}
