mod checkpoint;
mod criteria;
mod entries;
mod import;
mod meta;
mod project;
mod reads;

use uuid::Uuid;

use super::{CheckpointInput, MetaUpdate, StateFields};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

pub(super) struct Fixture {
    pub _db: TestDbHandle,
    pub handle: PersistHandle,
    pub project: i64,
}

pub(super) async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .define_knowledge_project("thread tests", Uuid::new_v4())
        .await?
        .id;
    Ok(Some(Fixture {
        _db: db,
        handle,
        project,
    }))
}

pub(super) fn input<'a>(slug: &'a str, tldr: &'a str) -> CheckpointInput<'a> {
    CheckpointInput {
        slug,
        tldr,
        state: StateFields::default(),
        session_uuid: None,
        commit_sha: None,
        contribution: None,
        meta: MetaUpdate::default(),
    }
}

pub(super) fn titled<'a>(slug: &'a str, tldr: &'a str) -> CheckpointInput<'a> {
    CheckpointInput {
        meta: MetaUpdate {
            title: Some("A thread"),
            ..MetaUpdate::default()
        },
        ..input(slug, tldr)
    }
}
