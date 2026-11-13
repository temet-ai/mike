use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::path::PathBuf;

use uuid::Uuid;

use super::import_repo;
use super::registry::RegistryEntry;
use super::report::Report;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::project_knowledge::LocationInput;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::work_threads::{HistoryPage, ListFilter};

fn fixture_repo() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/threads/repo")
        .to_string_lossy()
        .into_owned()
}

async fn handle_with_project(
    repo: &str,
) -> anyhow::Result<Option<(TestDbHandle, PersistHandle, i64)>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .knowledge()
        .define_knowledge_project("import tests", Uuid::new_v4())
        .await?;
    for root in [repo, "/Users/vimes/projects/temet/temet-proxy"] {
        handle
            .knowledge()
            .register_project_location(&LocationInput {
                project: project.project_uuid.to_string(),
                machine_uuid: Uuid::new_v4(),
                machine_name: "test machine".to_owned(),
                root_path: root.to_owned(),
            })
            .await?;
    }
    Ok(Some((db, handle, project.id)))
}

fn missing_entry(repo: &str) -> RegistryEntry {
    RegistryEntry {
        thread: "missing-from-disk".into(),
        repo: repo.into(),
        status: Some("review".into()),
        branch: Some("feat/x".into()),
        worktree: Some(format!("{repo}/platform/x")),
        raw: "{\"thread\": \"missing-from-disk\"}".into(),
    }
}

#[tokio::test]
async fn importing_the_fixture_repo_twice_creates_then_updates_without_duplicates()
-> anyhow::Result<()> {
    let repo = fixture_repo();
    let Some((_db, handle, project)) = handle_with_project(&repo).await? else {
        return Ok(());
    };
    let entry = missing_entry(&repo);
    let mut first = Report::default();
    import_repo(&handle, &repo, &[&entry], false, &mut first).await;
    assert_eq!(first.created.len(), 6, "{}", first.render(false));
    assert!(first.updated.is_empty());
    assert!(first.skipped_repos.is_empty());
    let errors: Vec<&str> = first
        .parse_errors
        .iter()
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|path| path.ends_with("no-title/worklog.md"))
    );
    assert!(
        errors
            .iter()
            .any(|path| path.ends_with("missing-from-disk/worklog.md"))
    );
    assert!(
        first.unresolved_dependencies.is_empty(),
        "{:?}",
        first.unresolved_dependencies
    );

    let mut second = Report::default();
    import_repo(&handle, &repo, &[&entry], false, &mut second).await;
    assert!(second.created.is_empty(), "{}", second.render(false));
    assert_eq!(second.updated.len(), 6);

    let listed = handle
        .launch()
        .list_work_threads(project, ListFilter::default())
        .await?;
    assert_eq!(listed.total, 6);
    let motion = handle
        .launch()
        .get_work_thread(project, "frontend-motion", HistoryPage::everything())
        .await?;
    assert_eq!(
        motion.depends_on,
        ["branch-consolidation", "judgement-context"]
    );
    assert_eq!(
        motion.thread.worktree.as_deref(),
        Some(".claude/worktrees/motion")
    );
    assert_eq!(motion.history_total, 4);
    assert!(
        motion
            .thread
            .source_path
            .as_deref()
            .is_some_and(|p| p.ends_with("frontend-motion/worklog.md"))
    );
    let stub = handle
        .launch()
        .get_work_thread(project, "missing-from-disk", HistoryPage::default())
        .await?;
    assert_eq!(stub.thread.status.as_deref(), Some("review"));
    assert_eq!(stub.thread.worktree.as_deref(), Some("platform/x"));
    assert_eq!(stub.thread.title, None);
    let legacy = handle
        .launch()
        .get_work_thread(project, "legacy-labels", HistoryPage::default())
        .await?;
    assert_eq!(legacy.thread.title.as_deref(), Some("Legacy labels"));
    assert_eq!(legacy.thread.status.as_deref(), Some("done"));
    Ok(())
}

#[tokio::test]
async fn a_dry_run_reports_what_would_happen_and_writes_nothing() -> anyhow::Result<()> {
    let repo = fixture_repo();
    let Some((_db, handle, project)) = handle_with_project(&repo).await? else {
        return Ok(());
    };
    let mut report = Report::default();
    import_repo(&handle, &repo, &[], true, &mut report).await;
    assert_eq!(report.created.len(), 5, "{}", report.render(true));
    assert_eq!(
        handle
            .launch()
            .list_work_threads(project, ListFilter::default())
            .await?
            .total,
        0
    );
    let text = report.render(true);
    assert!(text.starts_with("Dry run: nothing was written to the database."));
    assert!(text.contains("Threads would be created: 5"));
    Ok(())
}

#[tokio::test]
async fn a_repository_no_project_claims_is_skipped_with_the_reason() -> anyhow::Result<()> {
    let repo = fixture_repo();
    let Some((_db, handle, _)) = handle_with_project(&repo).await? else {
        return Ok(());
    };
    let mut report = Report::default();
    import_repo(&handle, "/nowhere/at/all", &[], false, &mut report).await;
    assert_eq!(report.skipped_repos.len(), 1);
    assert!(
        report.skipped_repos[0]
            .1
            .contains("register_project_location")
    );
    assert!(report.created.is_empty());
    Ok(())
}
