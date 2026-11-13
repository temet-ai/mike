pub mod acceptance_notes;
pub mod carry;
pub mod comms_thread;
pub mod crud;
pub mod events;
pub mod for_session;
pub mod notes;
pub mod run_record;
pub mod seats;
pub mod seed;
pub mod session_memo;
pub mod state;
pub mod told;
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
