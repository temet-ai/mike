use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};

pub(crate) struct Fixture {
    _db: TestDbHandle,
    pub(crate) handle: PersistHandle,
    pub(crate) sentinel: i64,
    pub(crate) machine: Uuid,
}

pub(crate) struct Bound {
    pub(crate) project_id: i64,
    pub(crate) resolution: String,
    pub(crate) location: Option<Uuid>,
}

impl Fixture {
    pub(crate) async fn new() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let sentinel = sqlx::query_scalar(
            "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ('(no project)', '(no project)', now(), now())
             ON CONFLICT (project_path) DO UPDATE SET last_seen = now() RETURNING id",
        )
        .fetch_one(handle.pool())
        .await?;
        let machine = Uuid::new_v4();
        handle.ensure_machine(machine, "host").await?;
        Ok(Self {
            _db: db,
            handle,
            sentinel,
            machine,
        })
    }

    pub(crate) async fn project(&self, path: &str) -> anyhow::Result<i64> {
        seed_project(self.handle.pool(), path).await
    }

    pub(crate) async fn location(&self, project: i64, root: &str) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO projects.locations(project_id, machine_uuid, root_path)
             VALUES ($1, $2, $3) RETURNING location_uuid",
        )
        .bind(project)
        .bind(self.machine)
        .bind(root)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(crate) async fn unresolved(&self, cwd: Option<&str>) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO sessions.sessions(session_uuid, project_id, cwd, machine_uuid, first_seen, last_seen)
             VALUES (gen_random_uuid(), $1, $2, $3, now(), now()) RETURNING session_uuid",
        )
        .bind(self.sentinel)
        .bind(cwd)
        .bind(self.machine)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(crate) async fn resolved(
        &self,
        project: i64,
        resolution: &str,
        location: Option<Uuid>,
    ) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO sessions.sessions(session_uuid, project_id, project_resolution, location_uuid, first_seen, last_seen)
             VALUES (gen_random_uuid(), $1, $2, $3, now(), now()) RETURNING session_uuid",
        )
        .bind(project)
        .bind(resolution)
        .bind(location)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(crate) async fn bound(&self, session: Uuid) -> anyhow::Result<Bound> {
        let (project_id, resolution, location): (i64, String, Option<Uuid>) = sqlx::query_as(
            "SELECT project_id, project_resolution, location_uuid FROM sessions.sessions WHERE session_uuid = $1",
        )
        .bind(session)
        .fetch_one(self.handle.pool())
        .await?;
        Ok(Bound {
            project_id,
            resolution,
            location,
        })
    }
}

#[tokio::test]
async fn adopt_parent_bound_to_location_binds_child_to_that_location() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let project = fx.project("/repo/ask-kit").await?;
    let location = fx.location(project, "/repo/ask-kit").await?;
    let parent = fx.resolved(project, "operator", Some(location)).await?;
    let child = fx
        .unresolved(Some(&format!("/home/u/.claude/.cache/tmp/claude-1000/-repo-ask-kit/{parent}/scratchpad/x")))
        .await?;

    let adopted = fx.handle.adopt_parent_session_project(child, parent).await?;

    assert_eq!(adopted.map(|p| p.id), Some(project));
    let bound = fx.bound(child).await?;
    assert_eq!(bound.project_id, project);
    assert_eq!(bound.location, Some(location));
    assert_eq!(bound.resolution, "operator");
    Ok(())
}

#[tokio::test]
async fn adopt_parent_without_location_copies_project_and_resolution() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let project = fx.project("/repo/tether").await?;
    let parent = fx.resolved(project, "git", None).await?;
    let child = fx.unresolved(None).await?;

    fx.handle.adopt_parent_session_project(child, parent).await?;

    let bound = fx.bound(child).await?;
    assert_eq!(bound.project_id, project);
    assert_eq!(bound.resolution, "git");
    assert_eq!(bound.location, None);
    Ok(())
}

#[tokio::test]
async fn adopt_parent_unresolved_parent_leaves_child_unresolved() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let parent = fx.unresolved(None).await?;
    let child = fx.unresolved(None).await?;

    let adopted = fx.handle.adopt_parent_session_project(child, parent).await?;

    assert!(adopted.is_none());
    let bound = fx.bound(child).await?;
    assert_eq!(bound.project_id, fx.sentinel);
    assert_eq!(bound.resolution, "unresolved");
    Ok(())
}

#[tokio::test]
async fn adopt_parent_child_operator_assigned_is_unchanged() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let mine = fx.project("/repo/mine").await?;
    let theirs = fx.project("/repo/theirs").await?;
    let parent = fx.resolved(theirs, "operator", None).await?;
    let child = fx.resolved(mine, "operator", None).await?;

    let adopted = fx.handle.adopt_parent_session_project(child, parent).await?;

    assert!(adopted.is_none());
    assert_eq!(fx.bound(child).await?.project_id, mine);
    Ok(())
}

#[tokio::test]
async fn local_root_under_folder_of_projects_stays_unresolved() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let folder = fx.project("/data/projects").await?;
    fx.location(folder, "/data/projects").await?;
    let repo = fx.project("/data/projects/repo").await?;
    fx.location(repo, "/data/projects/repo").await?;
    let session = fx.unresolved(Some("/data/projects/other")).await?;

    let bound = fx
        .handle
        .bind_session_to_local_root(session, fx.machine, "/data/projects/other")
        .await?;

    assert!(bound.is_none());
    assert_eq!(fx.bound(session).await?.resolution, "unresolved");
    Ok(())
}

#[tokio::test]
async fn local_root_exact_folder_of_projects_binds_it() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let folder = fx.project("/data/projects").await?;
    let location = fx.location(folder, "/data/projects").await?;
    let repo = fx.project("/data/projects/repo").await?;
    fx.location(repo, "/data/projects/repo").await?;
    let session = fx.unresolved(Some("/data/projects")).await?;

    fx.handle
        .bind_session_to_local_root(session, fx.machine, "/data/projects")
        .await?;

    assert_eq!(fx.bound(session).await?.location, Some(location));
    Ok(())
}

#[tokio::test]
async fn local_root_subdir_of_checkout_with_own_worktree_binds_checkout() -> anyhow::Result<()> {
    let fx = Fixture::new().await?;
    let repo = fx.project("/data/repo").await?;
    let main = fx.location(repo, "/data/repo").await?;
    fx.location(repo, "/data/repo/.claude/worktrees/a").await?;
    let session = fx.unresolved(Some("/data/repo/crates")).await?;

    fx.handle
        .bind_session_to_local_root(session, fx.machine, "/data/repo/crates")
        .await?;

    assert_eq!(fx.bound(session).await?.location, Some(main));
    Ok(())
}
