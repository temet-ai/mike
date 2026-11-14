#[cfg(test)]
pub mod acceptance_notes;
#[cfg(test)]
pub mod adopt;
#[cfg(test)]
pub mod comms_thread;
#[cfg(test)]
pub mod crud;
#[cfg(test)]
pub mod events;
#[cfg(test)]
pub mod for_session;
#[cfg(test)]
pub mod notes;
#[cfg(test)]
pub mod run_record;
#[cfg(test)]
pub mod seats;
pub mod seed;
#[cfg(test)]
pub mod session_memo;
#[cfg(test)]
pub mod state;
#[cfg(test)]
pub mod told;
#[cfg(test)]
pub mod tripwires;

use serde_json::json;

use super::NewFleet;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{self, TestDbHandle};

pub struct Fixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
}

pub async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture { db, handle }))
}

pub fn new_fleet(slug: &str, project_path: &str) -> NewFleet {
    NewFleet {
        slug: slug.to_owned(),
        project_path: project_path.to_owned(),
        definition: json!({"seats": ["coordinator"]}),
        missing: json!([{"item": "criteria", "problem": "none written"}]),
        created_by: None,
    }
}
