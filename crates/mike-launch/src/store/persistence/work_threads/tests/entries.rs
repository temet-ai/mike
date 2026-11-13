use crate::store::persistence::launch_repo::LaunchPersist as _;
use serde_json::json;
use uuid::Uuid;

use super::super::reads::HistoryPage;
use super::super::{EntryInput, WorkThreadError};
use super::{fixture, titled};

fn entry<'a>(slug: &'a str, kind: &'a str, body: &'a str) -> EntryInput<'a> {
    EntryInput {
        slug,
        kind,
        body,
        summary: None,
        session_uuid: None,
        commit_sha: None,
        data: None,
    }
}

#[tokio::test]
async fn a_brief_is_appended_with_the_next_number_and_links_its_session() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .launch()
        .checkpoint_work_thread(f.project, &titled("brief", "start"))
        .await?;
    let session = Uuid::new_v4();
    let payload = json!({"for": "next session"});
    let outcome = f
        .handle
        .launch()
        .add_work_thread_entry(
            f.project,
            &EntryInput {
                session_uuid: Some(session),
                commit_sha: Some("def5678"),
                data: Some(&payload),
                ..entry(
                    "brief",
                    "brief",
                    "Read the plan first.\nThen build the importer.",
                )
            },
        )
        .await?;
    assert_eq!(outcome.entry.ord, 2);
    assert_eq!(outcome.entry.summary, "Read the plan first.");
    assert_eq!(outcome.entry.kind, "brief");
    let detail = f
        .handle
        .launch()
        .get_work_thread(f.project, "brief", HistoryPage::everything())
        .await?;
    assert_eq!(
        detail.history[0].body,
        "Read the plan first.\nThen build the importer."
    );
    assert_eq!(detail.history[0].data, Some(payload));
    assert_eq!(detail.sessions.len(), 1);
    assert_eq!(detail.sessions[0].session_uuid, session);
    assert_eq!(detail.thread.last_session_uuid, Some(session));
    Ok(())
}

#[tokio::test]
async fn system_kinds_blank_bodies_and_unknown_threads_are_refused() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .launch()
        .checkpoint_work_thread(f.project, &titled("guarded", "start"))
        .await?;
    let system = f
        .handle
        .launch()
        .add_work_thread_entry(f.project, &entry("guarded", "state-replaced", "forged"))
        .await;
    assert!(matches!(system, Err(WorkThreadError::Invalid(m)) if m.contains("state-replaced")));
    let blank = f
        .handle
        .launch()
        .add_work_thread_entry(f.project, &entry("guarded", "note", "  "))
        .await;
    assert!(matches!(blank, Err(WorkThreadError::Invalid(m)) if m.contains("body")));
    let unknown = f
        .handle
        .launch()
        .add_work_thread_entry(f.project, &entry("ghost", "note", "hello"))
        .await;
    assert!(matches!(unknown, Err(WorkThreadError::NotFound(m)) if m.contains("`ghost`")));
    Ok(())
}
