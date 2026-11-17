use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};

pub(crate) struct Fixture {
    _db: TestDbHandle,
    pub(crate) handle: PersistHandle,
    pub(crate) project: i64,
    pub(crate) machine: Uuid,
}

impl Fixture {
    pub(crate) async fn new() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let project = seed_project(handle.pool(), "/test/location-move").await?;
        let machine = Uuid::new_v4();
        handle.ensure_machine(machine, "host").await?;
        Ok(Self {
            _db: db,
            handle,
            project,
            machine,
        })
    }

    pub(crate) async fn location(&self, root: &str) -> anyhow::Result<Uuid> {
        self.location_of_project(self.project, root).await
    }

    pub(crate) async fn location_of_project(
        &self,
        project: i64,
        root: &str,
    ) -> anyhow::Result<Uuid> {
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

    pub(crate) async fn session(&self, location: Uuid) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO sessions.sessions(session_uuid, project_id, location_uuid, first_seen, last_seen)
             VALUES (gen_random_uuid(), $1, $2, now(), now()) RETURNING session_uuid",
        )
        .bind(self.project)
        .bind(location)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(crate) async fn source(
        &self,
        location: Uuid,
        name: &str,
        root: &str,
    ) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO memory.knowledge_sources(location_uuid, name, root_path)
             VALUES ($1, $2, $3) RETURNING source_uuid",
        )
        .bind(location)
        .bind(name)
        .bind(root)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(crate) async fn root_of(&self, location: Uuid) -> anyhow::Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT root_path FROM projects.locations WHERE location_uuid = $1")
                .bind(location)
                .fetch_optional(self.handle.pool())
                .await?,
        )
    }

    pub(crate) async fn session_location(&self, session: Uuid) -> anyhow::Result<Option<Uuid>> {
        Ok(sqlx::query_scalar(
            "SELECT location_uuid FROM sessions.sessions WHERE session_uuid = $1",
        )
        .bind(session)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn source_of(&self, source: Uuid) -> anyhow::Result<Option<(Uuid, String)>> {
        Ok(sqlx::query_as(
            "SELECT location_uuid, root_path FROM memory.knowledge_sources WHERE source_uuid = $1",
        )
        .bind(source)
        .fetch_optional(self.handle.pool())
        .await?)
    }

    async fn link(&self, child: Uuid, parent: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE projects.locations SET parent_location_uuid = $2 WHERE location_uuid = $1",
        )
        .bind(child)
        .bind(parent)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }
}

#[tokio::test]
async fn move_location_explicit_move_keeps_uuid_sessions_and_sources() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let main = f.location("/work/old").await?;
    let worktree = f.location("/work/old/.claude/worktrees/wt").await?;
    f.link(worktree, main).await?;
    let session = f.session(main).await?;
    let skills = f.source(main, "skills", "/work/old/.claude/skills").await?;
    let memory = f
        .source(
            main,
            "claude-memory",
            "/home/u/.claude/projects/-work-old/memory",
        )
        .await?;
    let tree_memory = f
        .source(
            worktree,
            "claude-memory",
            "/home/u/.claude/projects/-work-old--claude-worktrees-wt/memory",
        )
        .await?;
    let outside = f.source(main, "notes", "/elsewhere/notes").await?;

    let report = f.handle.move_location(main, "/work/new/").await?;

    assert_eq!(
        (report.location_uuid, report.from.as_str()),
        (main, "/work/old")
    );
    assert_eq!(report.to, "/work/new");
    assert_eq!(report.moved.len(), 2);
    assert!(report.merged.is_empty());
    assert_eq!(f.root_of(main).await?.as_deref(), Some("/work/new"));
    assert_eq!(
        f.root_of(worktree).await?.as_deref(),
        Some("/work/new/.claude/worktrees/wt")
    );
    assert_eq!(f.session_location(session).await?, Some(main));
    let expected = [
        (skills, "/work/new/.claude/skills"),
        (memory, "/home/u/.claude/projects/-work-new/memory"),
        (
            tree_memory,
            "/home/u/.claude/projects/-work-new--claude-worktrees-wt/memory",
        ),
        (outside, "/elsewhere/notes"),
    ];
    for (source, root) in expected {
        let found = f.source_of(source).await?.map(|(_, root)| root);
        assert_eq!(found.as_deref(), Some(root));
    }
    Ok(())
}

#[tokio::test]
async fn move_location_onto_existing_row_merges_it_into_the_moving_location() -> anyhow::Result<()>
{
    let f = Fixture::new().await?;
    let old = f.location("/work/old").await?;
    let existing = f.location("/work/new").await?;
    let kept_session = f.session(old).await?;
    let moved_session = f.session(existing).await?;
    let old_skills = f.source(old, "skills", "/work/old/.claude/skills").await?;
    let new_skills = f
        .source(existing, "skills", "/work/new/.claude/skills")
        .await?;
    let docs = f.source(existing, "docs", "/work/new/docs").await?;

    let report = f.handle.move_location(old, "/work/new").await?;

    assert_eq!(report.merged, vec![existing]);
    assert_eq!(report.sessions_repointed, 1);
    assert_eq!(f.root_of(old).await?.as_deref(), Some("/work/new"));
    assert_eq!(f.root_of(existing).await?, None);
    assert_eq!(f.session_location(kept_session).await?, Some(old));
    assert_eq!(f.session_location(moved_session).await?, Some(old));
    assert_eq!(
        f.source_of(old_skills).await?,
        Some((old, "/work/new/.claude/skills".to_owned()))
    );
    assert_eq!(f.source_of(new_skills).await?, None);
    assert_eq!(
        f.source_of(docs).await?,
        Some((old, "/work/new/docs".to_owned()))
    );
    Ok(())
}

#[tokio::test]
async fn move_location_onto_another_projects_location_is_refused() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let other = seed_project(f.handle.pool(), "/test/location-move-other").await?;
    let old = f.location("/work/old").await?;
    f.location_of_project(other, "/work/taken").await?;

    let Err(error) = f.handle.move_location(old, "/work/taken").await else {
        anyhow::bail!("a move onto another project's folder must be refused");
    };
    assert!(error.to_string().contains("/work/taken"), "{error}");
    let Err(error) = f.handle.move_location(old, "relative/path").await else {
        anyhow::bail!("a relative root must be refused");
    };
    assert!(error.to_string().contains("relative/path"), "{error}");
    assert_eq!(f.root_of(old).await?.as_deref(), Some("/work/old"));
    Ok(())
}
