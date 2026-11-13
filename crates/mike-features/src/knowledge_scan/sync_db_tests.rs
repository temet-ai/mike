use super::sync::{ScanMemo, encode_project_dir, scan_machine};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::{
    KnowledgeQuery, LocationInput, ModelKeys, ShownGuidance,
};
use crate::store::persistence::{
    PersistHandle,
    test_support::{acquire_ephemeral_db, seed_project},
};
use std::fs;
use std::path::Path;
use uuid::Uuid;

fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

async fn register(
    handle: &PersistHandle,
    project: &str,
    machine: Uuid,
    root: &Path,
) -> anyhow::Result<Uuid> {
    let row = handle
        .knowledge()
        .register_project_location(&LocationInput {
            project: project.into(),
            machine_uuid: machine,
            machine_name: format!("machine-{machine}"),
            root_path: root.to_string_lossy().into_owned(),
        })
        .await?;
    Ok(serde_json::from_value(row["location_uuid"].clone())?)
}

fn memory_file(home: &Path, root: &Path, name: &str) -> std::path::PathBuf {
    home.join("projects")
        .join(encode_project_dir(&root.to_string_lossy()))
        .join("memory")
        .join(name)
}

async fn live_documents(handle: &PersistHandle, source: &str) -> anyhow::Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT d.relative_path FROM memory.knowledge_documents d
           JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
          WHERE s.name = $1 AND d.deleted_at IS NULL ORDER BY d.relative_path",
    )
    .bind(source)
    .fetch_all(handle.pool())
    .await?)
}

#[tokio::test]
async fn scan_machine_indexes_memory_and_skills_for_this_machines_locations_only()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/scan-a").await?;
    let home = tempfile::tempdir()?;
    let mine = tempfile::tempdir()?;
    let theirs = tempfile::tempdir()?;
    let this_machine = Uuid::new_v4();
    register(&handle, "/test/scan-a", this_machine, mine.path()).await?;
    register(&handle, "/test/scan-a", Uuid::new_v4(), theirs.path()).await?;
    write(
        &memory_file(home.path(), mine.path(), "feedback-tests.md"),
        "---\nname: feedback-tests\ndescription: run the tests\nmetadata:\n  type: feedback\n---\nRun the tests before claiming they pass.",
    )?;
    write(
        &memory_file(home.path(), mine.path(), "MEMORY.md"),
        "- [feedback-tests](feedback-tests.md)",
    )?;
    write(
        &mine.path().join(".claude/skills/alpha/SKILL.md"),
        "---\nname: alpha\ndescription: alpha steps\n---\nDo alpha.",
    )?;
    write(
        &memory_file(home.path(), theirs.path(), "other.md"),
        "belongs to another machine",
    )?;

    let mut memo = ScanMemo::default();
    let first = scan_machine(&handle, this_machine, home.path(), &mut memo).await?;
    assert_eq!((first.scanned, first.updated, first.failed), (2, 2, 0));
    assert_eq!(
        live_documents(&handle, "claude-memory").await?,
        vec!["feedback-tests.md".to_owned()]
    );
    assert_eq!(
        live_documents(&handle, "skills").await?,
        vec!["alpha/SKILL.md".to_owned()]
    );

    let second = scan_machine(&handle, this_machine, home.path(), &mut memo).await?;
    assert_eq!((second.scanned, second.updated), (2, 0));

    write(
        &memory_file(home.path(), mine.path(), "feedback-tests.md"),
        "---\nname: feedback-tests\n---\nChanged text.",
    )?;
    write(
        &memory_file(home.path(), mine.path(), "second.md"),
        "A second memory.",
    )?;
    let third = scan_machine(&handle, this_machine, home.path(), &mut memo).await?;
    assert_eq!(third.updated, 1);
    let revisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.knowledge_revisions r
           JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid
          WHERE d.relative_path = 'feedback-tests.md'",
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(revisions, 2);

    fs::remove_file(memory_file(home.path(), mine.path(), "second.md"))?;
    let fourth = scan_machine(&handle, this_machine, home.path(), &mut memo).await?;
    assert_eq!(fourth.updated, 1);
    assert_eq!(
        live_documents(&handle, "claude-memory").await?,
        vec!["feedback-tests.md".to_owned()]
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn scan_machine_skips_locations_without_the_directories() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/scan-bare").await?;
    let home = tempfile::tempdir()?;
    let bare = tempfile::tempdir()?;
    let machine = Uuid::new_v4();
    register(&handle, "/test/scan-bare", machine, bare.path()).await?;
    let report = scan_machine(&handle, machine, home.path(), &mut ScanMemo::default()).await?;
    assert_eq!((report.scanned, report.updated, report.failed), (0, 0, 0));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn scan_machine_records_a_failure_and_keeps_the_index_when_a_file_is_not_utf8()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/scan-bad").await?;
    let home = tempfile::tempdir()?;
    let root = tempfile::tempdir()?;
    let machine = Uuid::new_v4();
    register(&handle, "/test/scan-bad", machine, root.path()).await?;
    write(
        &memory_file(home.path(), root.path(), "good.md"),
        "good text",
    )?;
    let mut memo = ScanMemo::default();
    scan_machine(&handle, machine, home.path(), &mut memo).await?;
    fs::write(
        memory_file(home.path(), root.path(), "bad.md"),
        [0xff, 0xfe],
    )?;
    let report = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!((report.failed, report.updated), (1, 0));
    assert_eq!(
        live_documents(&handle, "claude-memory").await?,
        vec!["good.md".to_owned()]
    );
    let failure: Option<String> = sqlx::query_scalar(
        "SELECT last_failure FROM memory.knowledge_sources WHERE name = 'claude-memory'",
    )
    .fetch_one(handle.pool())
    .await?;
    assert!(failure.is_some_and(|text| text.contains("bad.md")));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn closest_guidance_finds_memory_by_text_and_records_what_was_cited() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let knowledge_db = handle.knowledge();
    let project_id = seed_project(handle.pool(), "/test/scan-find").await?;
    let other_id = seed_project(handle.pool(), "/test/scan-other").await?;
    let home = tempfile::tempdir()?;
    let root = tempfile::tempdir()?;
    let worktree = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    let machine = Uuid::new_v4();
    register(&handle, "/test/scan-find", machine, root.path()).await?;
    register(&handle, "/test/scan-find", machine, worktree.path()).await?;
    register(&handle, "/test/scan-other", machine, other.path()).await?;
    let skill = "---\nname: worktree-rules\ndescription: where worktrees live\n---\nKeep every worktree inside the repository.";
    write(
        &root.path().join(".claude/skills/worktree-rules/SKILL.md"),
        skill,
    )?;
    write(
        &worktree
            .path()
            .join(".claude/skills/worktree-rules/SKILL.md"),
        skill,
    )?;
    write(
        &memory_file(home.path(), root.path(), "feedback-tests.md"),
        "---\nname: feedback-tests\n---\nAlways run the test suite before claiming the tests pass.",
    )?;
    write(
        &memory_file(home.path(), other.path(), "foreign.md"),
        "Always run the test suite before claiming the tests pass in the other project.",
    )?;
    scan_machine(&handle, machine, home.path(), &mut ScanMemo::default()).await?;

    let rows = knowledge_db
        .closest_guidance(
            project_id,
            "you claimed the tests pass without running the test suite",
            None,
            3,
        )
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].relative_path, "feedback-tests.md");
    assert_eq!(rows[0].kind_label(), "memory");
    assert!(rows[0].body.contains("Always run the test suite"));

    let skills = knowledge_db
        .closest_guidance(project_id, "worktree inside the repository", None, 3)
        .await?;
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].kind_label(), "skill");
    assert_eq!(skills[0].display_name(), "worktree-rules");
    assert!(
        knowledge_db
            .closest_guidance(project_id, "   ", None, 3)
            .await?
            .is_empty()
    );
    assert!(
        knowledge_db
            .closest_guidance(other_id, "worktree inside the repository", None, 3)
            .await?
            .is_empty()
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn proposal_guidance_keeps_one_row_per_correction_and_search_reports_memory_kind()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let knowledge_db = handle.knowledge();
    let project_id = seed_project(handle.pool(), "/test/scan-find").await?;
    let home = tempfile::tempdir()?;
    let root = tempfile::tempdir()?;
    let machine = Uuid::new_v4();
    register(&handle, "/test/scan-find", machine, root.path()).await?;
    write(
        &memory_file(home.path(), root.path(), "feedback-tests.md"),
        "---\nname: feedback-tests\n---\nAlways run the test suite before claiming the tests pass.",
    )?;
    scan_machine(&handle, machine, home.path(), &mut ScanMemo::default()).await?;
    let rows = knowledge_db
        .closest_guidance(project_id, "run the test suite", None, 3)
        .await?;
    assert_eq!(rows.len(), 1);

    let verdict: i64 = 41;
    knowledge_db
        .record_proposal_guidance(
            verdict,
            project_id,
            &[ShownGuidance {
                row: &rows[0],
                cited: rows[0].cited_by("already stated in feedback-tests.md"),
            }],
        )
        .await?;
    knowledge_db
        .record_proposal_guidance(
            verdict,
            project_id,
            &[ShownGuidance {
                row: &rows[0],
                cited: true,
            }],
        )
        .await?;
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE cited) FROM memory.lesson_proposal_guidance
          WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(counts, (1, 1));

    let found = knowledge_db
        .search_project_knowledge(&KnowledgeQuery {
            project: "/test/scan-find".into(),
            query: "run the test suite".into(),
            kind: Some("memory".into()),
            ..KnowledgeQuery::default()
        })
        .await?;
    assert_eq!(found["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(found["items"][0]["source_kind"], "memory");
    db.cleanup().await?;
    Ok(())
}

fn axis_literal(axis: usize) -> String {
    let mut vector = vec![0.0_f32; 768];
    if let Some(slot) = vector.get_mut(axis) {
        *slot = 1.0;
    }
    crate::store::operator_turns::vector_to_text_literal(&vector)
}

#[tokio::test]
async fn closest_guidance_orders_by_vector_similarity_and_drops_unrelated_documents()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let knowledge_db = handle.knowledge();
    let project_id = seed_project(handle.pool(), "/test/scan-vector").await?;
    let home = tempfile::tempdir()?;
    let root = tempfile::tempdir()?;
    let machine = Uuid::new_v4();
    register(&handle, "/test/scan-vector", machine, root.path()).await?;
    write(
        &memory_file(home.path(), root.path(), "near.md"),
        "near text",
    )?;
    write(&memory_file(home.path(), root.path(), "far.md"), "far text")?;
    scan_machine(&handle, machine, home.path(), &mut ScanMemo::default()).await?;
    let docs: Vec<(i64, String)> = sqlx::query_as(
        "SELECT d.current_revision, d.relative_path FROM memory.knowledge_documents d",
    )
    .fetch_all(handle.pool())
    .await?;
    for (revision, path) in &docs {
        let axis = usize::from(path == "far.md");
        sqlx::query(
            "WITH state AS (
                 INSERT INTO memory.knowledge_embedding_state(source_kind, source_id, model_key, completed_at)
                 VALUES ('artifact', $1, 'model-a', now()) RETURNING id)
             INSERT INTO memory.knowledge_embedding_chunks(state_id, ordinal, body, embedding)
             SELECT id, 0, 'chunk', $2::public.vector FROM state",
        )
        .bind(revision)
        .bind(axis_literal(axis))
        .execute(handle.pool())
        .await?;
    }
    let vector = (
        axis_literal(0),
        ModelKeys::only("model-a"),
        "nomic-embed-text-v1.5".to_owned(),
    );
    let rows = knowledge_db
        .closest_guidance(project_id, "anything at all", Some(&vector), 3)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].relative_path, "near.md");
    assert!(rows[0].similarity.is_some_and(|s| s > 0.99));
    let other_model = (
        axis_literal(0),
        ModelKeys::only("model-b"),
        "nomic-embed-text-v1.5".to_owned(),
    );
    let none = knowledge_db
        .closest_guidance(project_id, "zzzz qqqq", Some(&other_model), 3)
        .await?;
    assert!(none.is_empty());
    let unmeasured = (
        axis_literal(0),
        ModelKeys::only("model-a"),
        "all-minilm".to_owned(),
    );
    let every = knowledge_db
        .closest_guidance(project_id, "anything at all", Some(&unmeasured), 3)
        .await?;
    assert_eq!(every.len(), 2);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn scan_machine_indexes_declared_sources_and_records_a_vanished_root() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/scan-declared").await?;
    let home = tempfile::tempdir()?;
    let repo = tempfile::tempdir()?;
    let machine = Uuid::new_v4();
    let location = register(&handle, "/test/scan-declared", machine, repo.path()).await?;
    write(
        &repo.path().join(".mike/knowledge.json"),
        r#"{"version":1,"sources":[
            {"name":"docs","path":"docs","exclude":["knowledge"]},
            {"name":"artifacts","path":"artifacts"}]}"#,
    )?;
    write(
        &repo.path().join("docs/CONFIGURATION.md"),
        "---\ntitle: Configuration\nstatus: active\n---\nfirst text",
    )?;
    write(&repo.path().join("docs/knowledge/skip.md"), "excluded")?;
    write(&repo.path().join("docs/notes.txt"), "not markdown")?;

    let mut memo = ScanMemo::default();
    let first = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!((first.scanned, first.updated, first.failed), (1, 1, 1));
    assert_eq!(
        live_documents(&handle, "docs").await?,
        vec!["CONFIGURATION.md".to_owned()]
    );
    let failure = source_failure(&handle, location, "artifacts").await?;
    assert!(
        failure.is_some_and(|text| text.contains("does not exist")),
        "a declared root that is absent records a failure"
    );
    assert_eq!(source_failure(&handle, location, "skills").await?, None);

    let second = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!((second.updated, second.failed), (0, 0));

    write(
        &repo.path().join("docs/CONFIGURATION.md"),
        "---\ntitle: Configuration\nstatus: active\n---\nfirst text, now much longer",
    )?;
    let third = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!(third.updated, 1);
    let indexed: String = sqlx::query_scalar(
        "SELECT r.body FROM memory.knowledge_revisions r
           JOIN memory.knowledge_documents d ON d.current_revision = r.id
           JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
          WHERE s.location_uuid = $1 AND s.name = 'docs'",
    )
    .bind(location)
    .fetch_one(handle.pool())
    .await?;
    assert!(indexed.ends_with("now much longer"));

    fs::remove_dir_all(repo.path().join("docs"))?;
    let fourth = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!(fourth.failed, 1);
    assert!(
        source_failure(&handle, location, "docs")
            .await?
            .is_some_and(|text| text.contains("does not exist"))
    );
    assert_eq!(
        live_documents(&handle, "docs").await?,
        vec!["CONFIGURATION.md".to_owned()]
    );
    db.cleanup().await?;
    Ok(())
}

async fn source_failure(
    handle: &PersistHandle,
    location: Uuid,
    name: &str,
) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "SELECT last_failure FROM memory.knowledge_sources
          WHERE location_uuid = $1 AND name = $2 AND last_failure_generation = generation",
    )
    .bind(location)
    .bind(name)
    .fetch_optional(handle.pool())
    .await?
    .flatten())
}
