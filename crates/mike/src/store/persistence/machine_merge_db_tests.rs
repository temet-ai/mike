use uuid::Uuid;

use super::MachineMergeReport;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};

struct Fixture {
    _db: TestDbHandle,
    handle: PersistHandle,
    project: i64,
    legacy: Uuid,
    current: Uuid,
    other: Uuid,
}

async fn fixture() -> anyhow::Result<Fixture> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/test/machine-merge").await?;
    Ok(Fixture {
        _db: db,
        handle,
        project,
        legacy: Uuid::new_v4(),
        current: Uuid::new_v4(),
        other: Uuid::new_v4(),
    })
}

impl Fixture {
    async fn location(&self, machine: Uuid, root: &str, minutes_ago: i32) -> anyhow::Result<Uuid> {
        sqlx::query(
            "INSERT INTO projects.machines(machine_uuid, name) VALUES ($1, 'host')
             ON CONFLICT DO NOTHING",
        )
        .bind(machine)
        .execute(self.handle.pool())
        .await?;
        Ok(sqlx::query_scalar(
            "INSERT INTO projects.locations(project_id, machine_uuid, root_path, last_seen)
             VALUES ($1, $2, $3, now() - make_interval(mins => $4)) RETURNING location_uuid",
        )
        .bind(self.project)
        .bind(machine)
        .bind(root)
        .bind(minutes_ago)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn session(&self, location: Uuid) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO sessions.sessions(session_uuid, project_id, location_uuid, first_seen, last_seen)
             VALUES (gen_random_uuid(), $1, $2, now(), now()) RETURNING session_uuid",
        )
        .bind(self.project)
        .bind(location)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn source(&self, location: Uuid, name: &str) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO memory.knowledge_sources(location_uuid, name, root_path)
             VALUES ($1, $2, '/x') RETURNING source_uuid",
        )
        .bind(location)
        .bind(name)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn document(&self, source: Uuid, path: &str, body: &str) -> anyhow::Result<()> {
        let document: Uuid = sqlx::query_scalar(
            "INSERT INTO memory.knowledge_documents(source_uuid, relative_path)
             VALUES ($1, $2) RETURNING document_uuid",
        )
        .bind(source)
        .bind(path)
        .fetch_one(self.handle.pool())
        .await?;
        sqlx::query(
            "WITH r AS (
               INSERT INTO memory.knowledge_revisions(document_uuid, title, body, content_hash, kind)
               VALUES ($1, $2, $2, md5($2), 'note') RETURNING id)
             UPDATE memory.knowledge_documents SET current_revision = r.id FROM r
              WHERE document_uuid = $1",
        )
        .bind(document)
        .bind(body)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    async fn receipt(&self, location: Uuid, lesson: Uuid, version: i64) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO memory.lesson_file_changes(lesson_uuid, project_id, version, payload)
             VALUES ($1, $2, $3, '{}'::jsonb) ON CONFLICT DO NOTHING",
        )
        .bind(lesson)
        .bind(self.project)
        .bind(version)
        .execute(self.handle.pool())
        .await?;
        sqlx::query(
            "INSERT INTO memory.lesson_file_receipts(location_uuid, lesson_uuid, version)
             VALUES ($1, $2, $3)",
        )
        .bind(location)
        .bind(lesson)
        .bind(version)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    async fn conflict(&self, location: Uuid, lesson: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO memory.lesson_file_conflicts(location_uuid, lesson_uuid, server_version)
             VALUES ($1, $2, 5)",
        )
        .bind(location)
        .bind(lesson)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    async fn llm_rows(&self, machine: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO llm.calls(role, model, outcome, machine_uuid)
             VALUES ('steerer', 'm', 'ok', $1)",
        )
        .bind(machine)
        .execute(self.handle.pool())
        .await?;
        sqlx::query("INSERT INTO llm.worker_leases(role, machine_uuid) VALUES ('eval', $1)")
            .bind(machine)
            .execute(self.handle.pool())
            .await?;
        Ok(())
    }

    async fn merge(&self) -> anyhow::Result<MachineMergeReport> {
        Ok(self
            .handle
            .merge_machine_identity(self.legacy, self.current, "host")
            .await?)
    }

    async fn roots(&self, machine: Uuid) -> anyhow::Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT root_path FROM projects.locations WHERE machine_uuid = $1 ORDER BY root_path",
        )
        .bind(machine)
        .fetch_all(self.handle.pool())
        .await?)
    }

    async fn location_of(&self, machine: Uuid, root: &str) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "SELECT location_uuid FROM projects.locations WHERE machine_uuid = $1 AND root_path = $2",
        )
        .bind(machine)
        .bind(root)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn count(&self, sql: &'static str, key: Uuid) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(sql)
            .bind(key)
            .fetch_one(self.handle.pool())
            .await?)
    }
}

struct Seeded {
    session: Uuid,
    lessons: [Uuid; 3],
}

struct Locations {
    legacy_a: Uuid,
    legacy_b: Uuid,
    current_a: Uuid,
}

async fn seed_locations(f: &Fixture) -> anyhow::Result<Locations> {
    let legacy_a = f.location(f.legacy, "/a", 10).await?;
    let legacy_b = f.location(f.legacy, "/b", 10).await?;
    let current_a = f.location(f.current, "/a", 0).await?;
    f.location(f.current, "/c", 0).await?;
    f.location(f.other, "/a", 0).await?;
    Ok(Locations {
        legacy_a,
        legacy_b,
        current_a,
    })
}

async fn seed_documents(f: &Fixture, at: &Locations) -> anyhow::Result<()> {
    let legacy_docs = f.source(at.legacy_a, "docs").await?;
    f.document(legacy_docs, "x.md", "legacy x").await?;
    f.document(legacy_docs, "y.md", "legacy y").await?;
    let current_docs = f.source(at.current_a, "docs").await?;
    f.document(current_docs, "x.md", "current x").await?;
    let only_b = f.source(at.legacy_b, "docs").await?;
    f.document(only_b, "z.md", "legacy z").await
}

async fn seed_lesson_files(f: &Fixture, at: &Locations) -> anyhow::Result<[Uuid; 3]> {
    let lessons = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    f.receipt(at.legacy_a, lessons[0], 5).await?;
    f.receipt(at.current_a, lessons[0], 3).await?;
    f.receipt(at.legacy_a, lessons[1], 1).await?;
    f.receipt(at.current_a, lessons[1], 2).await?;
    f.receipt(at.legacy_a, lessons[2], 7).await?;
    f.conflict(at.legacy_a, lessons[0]).await?;
    Ok(lessons)
}

async fn seed_colliding(f: &Fixture) -> anyhow::Result<Seeded> {
    let at = seed_locations(f).await?;
    let session = f.session(at.legacy_a).await?;
    seed_documents(f, &at).await?;
    let lessons = seed_lesson_files(f, &at).await?;
    f.llm_rows(f.legacy).await?;
    Ok(Seeded { session, lessons })
}

#[tokio::test]
async fn merge_machine_identity_collapses_colliding_locations_and_keeps_their_dependents()
-> anyhow::Result<()> {
    let f = fixture().await?;
    let seeded = seed_colliding(&f).await?;
    let report = f.merge().await?;
    assert_eq!(
        report,
        MachineMergeReport {
            locations_moved: 1,
            locations_merged: 1,
            sessions_repointed: 1,
            sources_merged: 1,
            documents_merged: 1,
            llm_calls_updated: 1,
            worker_leases_updated: 1,
        }
    );
    assert!(f.roots(f.legacy).await?.is_empty());
    assert_eq!(f.roots(f.current).await?, ["/a", "/b", "/c"]);
    assert_eq!(f.roots(f.other).await?, ["/a"]);
    assert_eq!(
        f.count(
            "SELECT count(*) FROM projects.machines WHERE machine_uuid = $1",
            f.legacy
        )
        .await?,
        0
    );
    let winner = f.location_of(f.current, "/a").await?;
    let session_location: Uuid =
        sqlx::query_scalar("SELECT location_uuid FROM sessions.sessions WHERE session_uuid = $1")
            .bind(seeded.session)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(session_location, winner);
    Ok(())
}

#[tokio::test]
async fn merge_machine_identity_keeps_every_revision_of_a_document_present_under_both_ids()
-> anyhow::Result<()> {
    let f = fixture().await?;
    seed_colliding(&f).await?;
    f.merge().await?;
    let winner = f.location_of(f.current, "/a").await?;
    let documents: Vec<(String, i64)> = sqlx::query_as(
        "SELECT d.relative_path, count(r.id)
           FROM memory.knowledge_documents d
           JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
           LEFT JOIN memory.knowledge_revisions r ON r.document_uuid = d.document_uuid
          WHERE s.location_uuid = $1
          GROUP BY d.relative_path ORDER BY d.relative_path",
    )
    .bind(winner)
    .fetch_all(f.handle.pool())
    .await?;
    assert_eq!(documents, [("x.md".to_owned(), 2), ("y.md".to_owned(), 1)]);
    let sources_at_a = f
        .count(
            "SELECT count(*) FROM memory.knowledge_sources WHERE location_uuid = $1",
            winner,
        )
        .await?;
    assert_eq!(sources_at_a, 1);
    let moved = f.location_of(f.current, "/b").await?;
    let sources_at_b = f
        .count(
            "SELECT count(*) FROM memory.knowledge_sources WHERE location_uuid = $1",
            moved,
        )
        .await?;
    assert_eq!(sources_at_b, 1);
    Ok(())
}

#[tokio::test]
async fn merge_machine_identity_keeps_the_newest_receipt_and_the_conflicts_of_both()
-> anyhow::Result<()> {
    let f = fixture().await?;
    let seeded = seed_colliding(&f).await?;
    f.merge().await?;
    let winner = f.location_of(f.current, "/a").await?;
    let receipts: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT lesson_uuid, version FROM memory.lesson_file_receipts WHERE location_uuid = $1",
    )
    .bind(winner)
    .fetch_all(f.handle.pool())
    .await?;
    let version = |lesson: Uuid| receipts.iter().find(|r| r.0 == lesson).map(|r| r.1);
    assert_eq!(receipts.len(), 3);
    assert_eq!(version(seeded.lessons[0]), Some(5));
    assert_eq!(version(seeded.lessons[1]), Some(2));
    assert_eq!(version(seeded.lessons[2]), Some(7));
    let conflicts = f
        .count(
            "SELECT count(*) FROM memory.lesson_file_conflicts WHERE location_uuid = $1",
            winner,
        )
        .await?;
    assert_eq!(conflicts, 1);
    Ok(())
}

#[tokio::test]
async fn merge_machine_identity_retags_llm_rows_of_the_legacy_id() -> anyhow::Result<()> {
    let f = fixture().await?;
    seed_colliding(&f).await?;
    f.merge().await?;
    let calls = f
        .count(
            "SELECT count(*) FROM llm.calls WHERE machine_uuid = $1",
            f.current,
        )
        .await?;
    let leases = f
        .count(
            "SELECT count(*) FROM llm.worker_leases WHERE machine_uuid = $1",
            f.current,
        )
        .await?;
    assert_eq!((calls, leases), (1, 1));
    Ok(())
}

#[tokio::test]
async fn merge_machine_identity_keeps_the_legacy_row_when_it_was_seen_last() -> anyhow::Result<()> {
    let f = fixture().await?;
    let legacy_a = f.location(f.legacy, "/a", 0).await?;
    f.location(f.current, "/a", 30).await?;
    let report = f.merge().await?;
    assert_eq!(report.locations_merged, 1);
    assert_eq!(f.location_of(f.current, "/a").await?, legacy_a);
    assert_eq!(f.roots(f.current).await?, ["/a"]);
    Ok(())
}

#[tokio::test]
async fn merge_machine_identity_twice_changes_nothing_the_second_time() -> anyhow::Result<()> {
    let f = fixture().await?;
    seed_colliding(&f).await?;
    f.merge().await?;
    let before = f.roots(f.current).await?;
    let again = f.merge().await?;
    assert_eq!(again, MachineMergeReport::default());
    assert_eq!(f.roots(f.current).await?, before);
    Ok(())
}

#[tokio::test]
async fn merge_machine_identity_without_legacy_rows_registers_only_the_current_machine()
-> anyhow::Result<()> {
    let f = fixture().await?;
    f.location(f.other, "/a", 0).await?;
    let report = f.merge().await?;
    assert_eq!(report, MachineMergeReport::default());
    assert_eq!(f.roots(f.other).await?, ["/a"]);
    let registered = f
        .count(
            "SELECT count(*) FROM projects.machines WHERE machine_uuid = $1",
            f.current,
        )
        .await?;
    assert_eq!(registered, 1);
    Ok(())
}
