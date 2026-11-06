use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::questions::AskedQuestion;
use crate::store::persistence::session_goals::SetSessionIdentity;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

pub(super) struct Fixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
}

pub(super) async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture { db, handle }))
}

pub(super) async fn session(handle: &PersistHandle) -> anyhow::Result<Uuid> {
    let uuid = Uuid::new_v4();
    handle.ensure_session_exists(&uuid.to_string()).await?;
    Ok(uuid)
}

pub(super) async fn named(handle: &PersistHandle, session: Uuid, name: &str) -> anyhow::Result<()> {
    handle
        .set_session_goal(SetSessionIdentity {
            session_uuid: session,
            display_name: Some(name),
            role: None,
            goal: Some("work"),
            next_steps: None,
            instructions: None,
            set_by: "test",
        })
        .await?;
    Ok(())
}

pub(super) async fn run(handle: &PersistHandle, run_id: &str) -> anyhow::Result<()> {
    handle
        .upsert_run(&NewRun {
            run_id: run_id.to_owned(),
            project_path: None,
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    Ok(())
}

pub(super) fn question<'a>(asker: Uuid, to: Uuid, text: &'a str) -> AskedQuestion<'a> {
    AskedQuestion {
        asker,
        to_session: Some(to),
        to_task: None,
        run_id: None,
        about_note: None,
        question: text,
        blocked: false,
    }
}
