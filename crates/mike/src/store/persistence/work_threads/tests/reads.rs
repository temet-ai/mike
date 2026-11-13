use super::super::reads::{HistoryPage, ListFilter};
use super::super::{CheckpointInput, EntryInput, MetaUpdate, StateFields, WorkThreadError};
use super::{fixture, input, titled};
use crate::store::persistence::launch_repo::LaunchPersist as _;

#[tokio::test]
async fn history_pages_newest_first_with_a_cursor_and_a_total() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .launch()
        .checkpoint_work_thread(f.project, &titled("paged", "one"))
        .await?;
    for body in ["two", "three", "four", "five"] {
        f.handle
            .launch()
            .add_work_thread_entry(
                f.project,
                &EntryInput {
                    slug: "paged",
                    kind: "note",
                    body,
                    summary: None,
                    session_uuid: None,
                    commit_sha: None,
                    data: None,
                },
            )
            .await?;
    }
    let first = f
        .handle
        .launch()
        .get_work_thread(
            f.project,
            "paged",
            HistoryPage {
                limit: 2,
                before_ord: None,
            },
        )
        .await?;
    let ords: Vec<i32> = first.history.iter().map(|e| e.ord).collect();
    assert_eq!(ords, [5, 4]);
    assert_eq!(first.history_total, 5);
    assert_eq!(first.history_next_before_ord, Some(4));
    let second = f
        .handle
        .launch()
        .get_work_thread(
            f.project,
            "paged",
            HistoryPage {
                limit: 2,
                before_ord: first.history_next_before_ord,
            },
        )
        .await?;
    let ords: Vec<i32> = second.history.iter().map(|e| e.ord).collect();
    assert_eq!(ords, [3, 2]);
    let last = f
        .handle
        .launch()
        .get_work_thread(
            f.project,
            "paged",
            HistoryPage {
                limit: 2,
                before_ord: Some(2),
            },
        )
        .await?;
    assert_eq!(last.history.len(), 1);
    assert_eq!(last.history_next_before_ord, None);
    let missing = f
        .handle
        .launch()
        .get_work_thread(f.project, "ghost", HistoryPage::default())
        .await;
    assert!(matches!(missing, Err(WorkThreadError::NotFound(_))));
    Ok(())
}

#[tokio::test]
async fn list_returns_tldr_and_next_up_filters_by_status_and_pages() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .launch()
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                state: StateFields {
                    next_up: "ship it",
                    ..StateFields::default()
                },
                ..titled("alpha", "alpha summary")
            },
        )
        .await?;
    f.handle
        .launch()
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                meta: MetaUpdate {
                    title: Some("Beta"),
                    status: Some("parked"),
                    ..MetaUpdate::default()
                },
                ..input("beta", "beta summary")
            },
        )
        .await?;
    let all = f
        .handle
        .launch()
        .list_work_threads(f.project, ListFilter::default())
        .await?;
    assert_eq!(all.total, 2);
    assert_eq!(all.threads[0].slug, "beta");
    let alpha = all
        .threads
        .iter()
        .find(|t| t.slug == "alpha")
        .ok_or_else(|| anyhow::anyhow!("alpha missing"))?;
    assert_eq!(alpha.tldr, "alpha summary");
    assert_eq!(alpha.next_up, "ship it");
    let parked = f
        .handle
        .launch()
        .list_work_threads(
            f.project,
            ListFilter {
                status: Some("parked"),
                ..ListFilter::default()
            },
        )
        .await?;
    assert_eq!(parked.total, 1);
    assert_eq!(parked.threads[0].slug, "beta");
    let either = f
        .handle
        .launch()
        .list_work_threads(
            f.project,
            ListFilter {
                status: Some("active, parked"),
                ..ListFilter::default()
            },
        )
        .await?;
    assert_eq!(either.total, 2);
    let page = f
        .handle
        .launch()
        .list_work_threads(
            f.project,
            ListFilter {
                limit: 1,
                offset: 1,
                status: None,
            },
        )
        .await?;
    assert_eq!(page.threads.len(), 1);
    assert_eq!(page.total, 2);
    let bad = f
        .handle
        .launch()
        .list_work_threads(
            f.project,
            ListFilter {
                status: Some("nonsense"),
                ..ListFilter::default()
            },
        )
        .await;
    assert!(matches!(bad, Err(WorkThreadError::Invalid(m)) if m.contains("nonsense")));
    Ok(())
}
