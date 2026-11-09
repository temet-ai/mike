use super::*;
use crate::store::persistence::{
    PersistError, PersistHandle,
    test_support::{TestDbHandle, acquire_ephemeral_db, seed_project, seed_two_sessions},
};
use sqlx::PgPool;
use uuid::Uuid;

struct World {
    db: TestDbHandle,
    handle: PersistHandle,
    sessions: [Uuid; 2],
}

impl World {
    async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let seed = seed_project(handle.pool(), "/test/binding-seed").await?;
        let sessions = [Uuid::new_v4(), Uuid::new_v4()];
        seed_two_sessions(
            handle.pool(),
            &sessions[0].to_string(),
            &sessions[1].to_string(),
            seed,
        )
        .await?;
        Ok(Self {
            db,
            handle,
            sessions,
        })
    }

    async fn project(&self, name: &str) -> anyhow::Result<ProjectIdentity> {
        Ok(self
            .handle
            .define_knowledge_project(name, Uuid::new_v4())
            .await?)
    }

    async fn location(
        &self,
        project: &ProjectIdentity,
        machine: Uuid,
        root: &str,
    ) -> anyhow::Result<Uuid> {
        let row = self
            .handle
            .register_project_location(&LocationInput {
                project: project.project_uuid.to_string(),
                machine_uuid: machine,
                machine_name: "machine".into(),
                root_path: root.into(),
            })
            .await?;
        Ok(serde_json::from_value(row["location_uuid"].clone())?)
    }

    async fn state(&self, session: Uuid) -> anyhow::Result<(Option<Uuid>, i64, String)> {
        session_state(self.handle.pool(), session).await
    }

    async fn location_rows(&self, project: &ProjectIdentity) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT count(*) FROM projects.locations WHERE project_id = $1")
                .bind(project.id)
                .fetch_one(self.handle.pool())
                .await?,
        )
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}

async fn session_state(
    pool: &PgPool,
    session: Uuid,
) -> anyhow::Result<(Option<Uuid>, i64, String)> {
    Ok(sqlx::query_as(
        "SELECT location_uuid, project_id, project_resolution
         FROM sessions.sessions WHERE session_uuid = $1",
    )
    .bind(session)
    .fetch_one(pool)
    .await?)
}

fn is_conflict(result: Result<impl std::fmt::Debug, PersistError>) -> bool {
    matches!(result, Err(PersistError::DataInvariant(_)))
}

#[tokio::test]
async fn session_binding_second_machine_with_a_different_path_resolves_the_same_project()
-> anyhow::Result<()> {
    let world = World::boot().await?;
    let project = world.project("Portable").await?;
    let desktop = world
        .location(&project, Uuid::new_v4(), "/home/dev/app")
        .await?;
    let laptop = world
        .location(&project, Uuid::new_v4(), "/Users/dev/code/app")
        .await?;
    assert_ne!(desktop, laptop);

    world
        .handle
        .bind_session_location(world.sessions[0], desktop)
        .await?;
    world
        .handle
        .bind_session_location(world.sessions[1], laptop)
        .await?;

    assert_eq!(
        world.state(world.sessions[0]).await?,
        (Some(desktop), project.id, "operator".to_string())
    );
    assert_eq!(
        world.state(world.sessions[1]).await?,
        (Some(laptop), project.id, "operator".to_string())
    );
    assert_eq!(world.location_rows(&project).await?, 2);
    world.cleanup().await
}

#[tokio::test]
async fn session_binding_while_a_capture_holds_a_foreign_key_lock_completes() -> anyhow::Result<()>
{
    let world = World::boot().await?;
    let project = world.project("Captured").await?;
    let location = world
        .location(&project, Uuid::new_v4(), "/home/dev/app")
        .await?;
    let mut capture = world.handle.pool().begin().await?;
    sqlx::query("SELECT 1 FROM sessions.sessions WHERE session_uuid = $1 FOR KEY SHARE")
        .bind(world.sessions[0])
        .execute(&mut *capture)
        .await?;

    let bound = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        world
            .handle
            .bind_session_location(world.sessions[0], location),
    )
    .await;
    capture.rollback().await?;

    assert!(
        bound.is_ok(),
        "bind waited on the foreign-key lock a capture transaction holds"
    );
    assert_eq!(bound?.map_err(anyhow::Error::from)?.id, project.id);
    assert_eq!(
        world.state(world.sessions[0]).await?,
        (Some(location), project.id, "operator".to_string())
    );
    world.cleanup().await
}

#[tokio::test]
async fn session_binding_repeated_location_is_idempotent() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let project = world.project("Idempotent").await?;
    let machine = Uuid::new_v4();
    let first = world.location(&project, machine, "/work/app").await?;
    let again = world.location(&project, machine, "/work/app").await?;
    assert_eq!(first, again);
    assert_eq!(world.location_rows(&project).await?, 1);

    let session = world.sessions[0];
    world.handle.bind_session_location(session, first).await?;
    let bound = world.state(session).await?;
    world.handle.bind_session_location(session, first).await?;
    world.handle.bind_session_location(session, again).await?;

    assert_eq!(world.state(session).await?, bound);
    assert_eq!(bound, (Some(first), project.id, "operator".to_string()));
    assert_eq!(world.location_rows(&project).await?, 1);
    world.cleanup().await
}

#[tokio::test]
async fn session_binding_location_of_another_project_is_refused_and_binding_kept()
-> anyhow::Result<()> {
    let world = World::boot().await?;
    let first = world.project("First").await?;
    let second = world.project("Second").await?;
    let machine = Uuid::new_v4();
    let owned = world.location(&first, machine, "/work/first").await?;
    let foreign = world.location(&second, machine, "/work/second").await?;
    let session = world.sessions[0];
    world.handle.bind_session_location(session, owned).await?;

    assert!(is_conflict(
        world.handle.bind_session_location(session, foreign).await
    ));

    assert_eq!(
        world.state(session).await?,
        (Some(owned), first.id, "operator".to_string())
    );
    world.cleanup().await
}

#[tokio::test]
async fn session_binding_registering_a_root_owned_by_another_project_is_refused()
-> anyhow::Result<()> {
    let world = World::boot().await?;
    let owner = world.project("Owner").await?;
    let intruder = world.project("Intruder").await?;
    let machine = Uuid::new_v4();
    let location = world.location(&owner, machine, "/work/shared").await?;

    let attempt = world
        .handle
        .register_project_location(&LocationInput {
            project: intruder.project_uuid.to_string(),
            machine_uuid: machine,
            machine_name: "machine".into(),
            root_path: "/work/shared".into(),
        })
        .await;

    assert!(is_conflict(attempt));
    assert_eq!(
        world.handle.project_for_location(location).await?.id,
        owner.id
    );
    assert_eq!(world.location_rows(&intruder).await?, 0);
    world.cleanup().await
}

#[tokio::test]
async fn local_root_binding_picks_the_longest_matching_root() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let outer = world.project("Outer").await?;
    let inner = world.project("Inner").await?;
    let machine = Uuid::new_v4();
    world.location(&outer, machine, "/work").await?;
    world.location(&inner, machine, "/work/app").await?;
    let [nested, sibling] = world.sessions;

    let bound = world
        .handle
        .bind_session_to_local_root(nested, machine, "/work/app/crates/mike")
        .await?;
    let other = world
        .handle
        .bind_session_to_local_root(sibling, machine, "/work/other")
        .await?;

    assert_eq!(bound.map(|p| p.id), Some(inner.id));
    assert_eq!(other.map(|p| p.id), Some(outer.id));
    assert_eq!(world.state(nested).await?.1, inner.id);
    assert_eq!(world.state(sibling).await?.1, outer.id);
    world.cleanup().await
}

#[tokio::test]
async fn local_root_binding_matches_the_root_itself_and_a_trailing_slash() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let project = world.project("Exact").await?;
    let machine = Uuid::new_v4();
    let location = world.location(&project, machine, "/work/app/").await?;
    let [exact, slashed] = world.sessions;

    let at_root = world
        .handle
        .bind_session_to_local_root(exact, machine, "/work/app")
        .await?;
    let with_slash = world
        .handle
        .bind_session_to_local_root(slashed, machine, "/work/app/")
        .await?;

    assert_eq!(at_root.map(|p| p.id), Some(project.id));
    assert_eq!(with_slash.map(|p| p.id), Some(project.id));
    assert_eq!(world.state(exact).await?.0, Some(location));
    world.cleanup().await
}

#[tokio::test]
async fn local_root_binding_does_not_match_a_sibling_directory_sharing_the_prefix()
-> anyhow::Result<()> {
    let world = World::boot().await?;
    let project = world.project("Boundary").await?;
    let machine = Uuid::new_v4();
    world.location(&project, machine, "/work/app").await?;
    let session = world.sessions[0];
    let before = world.state(session).await?;

    let bound = world
        .handle
        .bind_session_to_local_root(session, machine, "/work/app-two/src")
        .await?;

    assert!(bound.is_none());
    assert_eq!(world.state(session).await?, before);
    world.cleanup().await
}

#[tokio::test]
async fn local_root_binding_ignores_roots_registered_by_other_machines() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let project = world.project("Elsewhere").await?;
    world
        .location(&project, Uuid::new_v4(), "/work/app")
        .await?;
    let session = world.sessions[0];
    let before = world.state(session).await?;

    let bound = world
        .handle
        .bind_session_to_local_root(session, Uuid::new_v4(), "/work/app/src")
        .await?;

    assert!(bound.is_none());
    assert_eq!(world.state(session).await?, before);
    world.cleanup().await
}

#[tokio::test]
async fn local_root_binding_leaves_an_already_bound_session_alone() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let owner = world.project("Owner").await?;
    let local = world.project("Local").await?;
    let machine = Uuid::new_v4();
    let owned = world
        .location(&owner, Uuid::new_v4(), "/Users/dev/app")
        .await?;
    world.location(&local, machine, "/work/app").await?;
    let session = world.sessions[0];
    world.handle.bind_session_location(session, owned).await?;

    let bound = world
        .handle
        .bind_session_to_local_root(session, machine, "/work/app/src")
        .await?;

    assert!(bound.is_none());
    assert_eq!(
        world.state(session).await?,
        (Some(owned), owner.id, "operator".to_string())
    );
    world.cleanup().await
}

#[tokio::test]
async fn local_root_binding_respects_an_operator_project_assignment() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let local = world.project("Local").await?;
    let machine = Uuid::new_v4();
    world.location(&local, machine, "/work/app").await?;
    let session = world.sessions[0];
    world
        .handle
        .assign_session_project(session, "/test/operator-choice")
        .await
        .map_err(|error| anyhow::anyhow!("assign failed: {error:?}"))?;
    let before = world.state(session).await?;
    assert_eq!(before.2, "operator");

    let bound = world
        .handle
        .bind_session_to_local_root(session, machine, "/work/app/src")
        .await?;

    assert!(bound.is_none());
    assert_eq!(world.state(session).await?, before);
    world.cleanup().await
}

#[tokio::test]
async fn local_root_binding_unknown_session_is_none() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let project = world.project("Unknown").await?;
    let machine = Uuid::new_v4();
    world.location(&project, machine, "/work/app").await?;

    let bound = world
        .handle
        .bind_session_to_local_root(Uuid::new_v4(), machine, "/work/app")
        .await?;

    assert!(bound.is_none());
    world.cleanup().await
}

#[tokio::test]
async fn session_binding_unknown_location_is_refused() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let attempt = world
        .handle
        .bind_session_location(world.sessions[0], Uuid::new_v4())
        .await;
    assert!(is_conflict(attempt));
    world.cleanup().await
}
