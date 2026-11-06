mod acceptance_notes;
mod carry;
mod comms_thread;
mod crud;
mod events;
mod for_session;
mod notes;
mod run_record;
mod seats;
mod seed;
mod session_memo;
mod state;
mod told;
mod tripwires;

use serde_json::json;

use super::NewFleet;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{self, TestDbHandle};

pub(super) struct Fixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
}

pub(super) async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture { db, handle }))
}

pub(super) fn new_fleet(slug: &str, project_path: &str) -> NewFleet {
    NewFleet {
        slug: slug.to_owned(),
        project_path: project_path.to_owned(),
        definition: json!({"seats": ["coordinator"]}),
        missing: json!([{"item": "criteria", "problem": "none written"}]),
        created_by: None,
    }
}
