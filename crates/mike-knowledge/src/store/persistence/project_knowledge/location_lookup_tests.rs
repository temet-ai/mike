use super::*;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::workspace::{PathMatch, ProjectPathResolution};
use uuid::Uuid;

struct World {
    db: TestDbHandle,
    handle: PersistHandle,
}

impl World {
    async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        Ok(Self { db, handle })
    }

    async fn located(
        &self,
        name: &str,
        machine: Uuid,
        root: &str,
    ) -> anyhow::Result<ProjectIdentity> {
        let project = self
            .handle
            .knowledge()
            .define_knowledge_project(name, Uuid::new_v4())
            .await?;
        self.handle
            .knowledge()
            .register_project_location(&LocationInput {
                project: project.project_uuid.to_string(),
                machine_uuid: machine,
                machine_name: "machine".into(),
                root_path: root.into(),
            })
            .await?;
        Ok(project)
    }

    async fn resolve(
        &self,
        path: &str,
        matching: PathMatch,
    ) -> anyhow::Result<ProjectPathResolution> {
        Ok(self.handle.resolve_project(path, matching).await?)
    }
}

#[tokio::test]
async fn resolve_project_by_a_registered_folder_returns_its_project() -> anyhow::Result<()> {
    let world = World::boot().await?;
    let machine = Uuid::new_v4();
    let outer = world.located("outer", machine, "/work/lookup").await?;
    let checkout = world
        .located("checkout", machine, "/work/lookup/app")
        .await?;

    for path in [
        "/work/lookup/app",
        "/work/lookup/app/",
        "/work/lookup/app/crates/core",
    ] {
        match world.resolve(path, PathMatch::OrSuffix).await? {
            ProjectPathResolution::Resolved(id, project_path) => {
                assert_eq!(id, checkout.id, "{path}");
                assert_eq!(project_path, checkout.project_path, "{path}");
            }
            other => anyhow::bail!("{path} resolved to {other:?}"),
        }
    }
    match world
        .resolve("/work/lookup/other", PathMatch::OrSuffix)
        .await?
    {
        ProjectPathResolution::Resolved(id, _) => assert_eq!(id, outer.id),
        other => anyhow::bail!("enclosing folder resolved to {other:?}"),
    }
    world.db.cleanup().await
}

#[tokio::test]
async fn resolve_project_by_folder_is_ambiguous_across_projects_and_skipped_for_exact()
-> anyhow::Result<()> {
    let world = World::boot().await?;
    world
        .located("first", Uuid::new_v4(), "/work/shared")
        .await?;
    world
        .located("second", Uuid::new_v4(), "/work/shared")
        .await?;

    assert!(matches!(
        world.resolve("/work/shared/src", PathMatch::OrSuffix).await?,
        ProjectPathResolution::Ambiguous(found) if found.len() == 2
    ));
    assert!(matches!(
        world.resolve("/work/shared", PathMatch::Exact).await?,
        ProjectPathResolution::NotFound
    ));
    assert!(matches!(
        world
            .resolve("/work/unregistered", PathMatch::OrSuffix)
            .await?,
        ProjectPathResolution::NotFound
    ));
    world.db.cleanup().await
}
