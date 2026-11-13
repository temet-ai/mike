use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::NaiveDate;

use super::super::reads::HistoryPage;
use super::super::{ImportOutcome, ImportedEntry, ImportedState, ImportedThread};
use super::{fixture, input};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 8).unwrap_or_default()
}

fn imported(slug: &str, entries: usize) -> ImportedThread {
    ImportedThread {
        slug: slug.to_owned(),
        title: Some("Imported thread".to_owned()),
        goal: Some("a goal".to_owned()),
        status: Some("implementing".to_owned()),
        branch: Some("main".to_owned()),
        worktree: None,
        last_session_uuid: None,
        source_path: format!("/repo/artifacts/threads/{slug}/worklog.md"),
        parse_error: None,
        state: Some(ImportedState {
            tldr: "where it stands".to_owned(),
            where_we_are: "where it stands".to_owned(),
            files: vec!["a/b.rs".to_owned()],
            ..ImportedState::default()
        }),
        acceptance_criteria: Some("A1. works".to_owned()),
        entries: (0..entries)
            .map(|index| ImportedEntry {
                date: date(),
                session_uuid: None,
                commit_sha: Some("abc1234".to_owned()),
                summary: format!("entry {index}"),
                body: format!("- 2026-09-08 entry {index}"),
            })
            .collect(),
        original_text: "the whole worklog".to_owned(),
        original_date: date(),
        session_uuids: Vec::new(),
        notes: vec!["worktree `x` is not a path".to_owned()],
    }
}

#[tokio::test]
async fn importing_twice_updates_in_place_and_keeps_the_whole_file_as_one_note()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread = imported("legacy", 3);
    assert_eq!(
        f.handle
            .launch()
            .import_work_thread(f.project, &thread)
            .await?,
        ImportOutcome::Created
    );
    assert_eq!(
        f.handle
            .launch()
            .import_work_thread(f.project, &thread)
            .await?,
        ImportOutcome::Updated
    );
    let detail = f
        .handle
        .launch()
        .get_work_thread(f.project, "legacy", HistoryPage::everything())
        .await?;
    assert_eq!(detail.history_total, 4);
    let note = detail
        .history
        .iter()
        .find(|e| e.kind == "note")
        .ok_or_else(|| anyhow::anyhow!("no note entry"))?;
    assert_eq!(note.body, "the whole worklog");
    let data = note
        .data
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("note has no data"))?;
    assert_eq!(
        data["imported_from"],
        "/repo/artifacts/threads/legacy/worklog.md"
    );
    assert_eq!(
        detail.thread.source_path.as_deref(),
        Some("/repo/artifacts/threads/legacy/worklog.md")
    );
    assert_eq!(detail.acceptance_criteria.as_deref(), Some("A1. works"));
    assert_eq!(
        detail.state.map(|s| s.tldr).as_deref(),
        Some("where it stands")
    );

    let shorter = imported("legacy", 1);
    f.handle
        .launch()
        .import_work_thread(f.project, &shorter)
        .await?;
    let detail = f
        .handle
        .launch()
        .get_work_thread(f.project, "legacy", HistoryPage::everything())
        .await?;
    assert_eq!(detail.history_total, 2);
    Ok(())
}

#[tokio::test]
async fn a_worklog_that_could_not_be_parsed_still_gets_a_row_with_its_error() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let broken = ImportedThread {
        title: None,
        status: None,
        state: None,
        acceptance_criteria: None,
        entries: Vec::new(),
        parse_error: Some("no title in the frontmatter".to_owned()),
        ..imported("broken", 0)
    };
    f.handle
        .launch()
        .import_work_thread(f.project, &broken)
        .await?;
    let detail = f
        .handle
        .launch()
        .get_work_thread(f.project, "broken", HistoryPage::default())
        .await?;
    assert_eq!(
        detail.thread.parse_error.as_deref(),
        Some("no title in the frontmatter")
    );
    assert_eq!(detail.thread.title, None);
    let neither = ImportedThread {
        title: None,
        parse_error: None,
        ..imported("neither", 0)
    };
    assert!(
        f.handle
            .launch()
            .import_work_thread(f.project, &neither)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn a_thread_changed_in_mike_after_import_is_not_overwritten_by_a_second_import()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .launch()
        .import_work_thread(f.project, &imported("moved", 1))
        .await?;
    f.handle
        .launch()
        .checkpoint_work_thread(f.project, &input("moved", "written in mike"))
        .await?;
    let again = f
        .handle
        .launch()
        .import_work_thread(f.project, &imported("moved", 1))
        .await?;
    assert!(matches!(again, ImportOutcome::LeftAlone(_)), "{again:?}");
    let detail = f
        .handle
        .launch()
        .get_work_thread(f.project, "moved", HistoryPage::default())
        .await?;
    assert_eq!(
        detail.state.map(|s| s.tldr).as_deref(),
        Some("written in mike")
    );
    Ok(())
}

#[tokio::test]
async fn imported_dependencies_link_known_threads_and_report_the_rest() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .launch()
        .import_work_thread(f.project, &imported("base-work", 0))
        .await?;
    f.handle
        .launch()
        .import_work_thread(f.project, &imported("top-work", 0))
        .await?;
    let wanted = vec![
        "base-work".to_owned(),
        "gone-work".to_owned(),
        "top-work".to_owned(),
    ];
    let unresolved = f
        .handle
        .launch()
        .link_imported_dependencies(f.project, "top-work", &wanted)
        .await?;
    assert_eq!(unresolved, ["gone-work"]);
    let detail = f
        .handle
        .launch()
        .get_work_thread(f.project, "top-work", HistoryPage::default())
        .await?;
    assert_eq!(detail.depends_on, ["base-work"]);
    Ok(())
}
