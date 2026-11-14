use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::seed_project;
use crate::store::persistence::work_threads::{
    CheckpointInput, EntryInput, HistoryEntry, MetaUpdate,
};
use crate::store::persistence::{PersistHandle, test_support};
use chrono::Utc;
use uuid::Uuid;

use super::{ASK, SHOWN, load, newest};
use crate::launch::fleet::FleetDefinition;

fn entry(ord: i32, kind: &str, summary: &str) -> HistoryEntry {
    HistoryEntry {
        ord,
        kind: kind.to_string(),
        entry_date: Utc::now().date_naive(),
        session_uuid: None,
        commit_sha: None,
        summary: summary.to_string(),
        body: String::new(),
        data: None,
        created_at: Utc::now(),
    }
}

#[test]
fn only_the_newest_findings_are_shown_each_with_its_entry_reference() -> anyhow::Result<()> {
    let mut history = vec![
        entry(1, "checkpoint", "state"),
        entry(2, "decision", "chose"),
    ];
    history.extend((3..12).map(|ord| entry(ord, "finding", &format!("finding {ord}"))));
    let shown = newest("fleet-runs", history).ok_or_else(|| anyhow::anyhow!("no findings"))?;
    assert_eq!(shown.findings.len(), SHOWN);
    assert_eq!(shown.findings[0].entry, "fleet-runs#11");
    assert_eq!(shown.findings[0].summary, "finding 11");
    assert_eq!(shown.findings[SHOWN - 1].entry, "fleet-runs#7");
    assert_eq!(shown.ask, ASK);
    Ok(())
}

#[test]
fn a_thread_with_no_findings_shows_nothing() {
    assert_eq!(
        newest("fleet-runs", vec![entry(1, "checkpoint", "state")]),
        None
    );
}

#[tokio::test]
async fn the_next_definition_sees_the_previous_runs_findings() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; previous runs DB test NOT RUN");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let path = format!("/tmp/previous-runs-{}", Uuid::new_v4());
    let project = seed_project(persist.pool(), &path).await?;
    let def = FleetDefinition::default();
    assert_eq!(load(&persist, &path, &def).await, None);
    persist
        .launch()
        .checkpoint_work_thread(
            project,
            &CheckpointInput {
                slug: "fleet-runs",
                tldr: "fleet runs",
                state: Default::default(),
                session_uuid: None,
                commit_sha: None,
                contribution: None,
                meta: MetaUpdate {
                    title: Some("Fleet runs"),
                    ..Default::default()
                },
            },
        )
        .await?;
    persist
        .launch()
        .add_work_thread_entry(
            project,
            &EntryInput {
                slug: "fleet-runs",
                kind: "finding",
                body: "details",
                summary: Some("open-issues: seats spent half their effort waiting on mail"),
                session_uuid: None,
                commit_sha: None,
                data: None,
            },
        )
        .await?;
    let shown = load(&persist, &path, &def)
        .await
        .ok_or_else(|| anyhow::anyhow!("findings not shown"))?;
    assert_eq!(shown.thread, "fleet-runs");
    assert_eq!(shown.findings.len(), 1);
    assert!(shown.findings[0].summary.starts_with("open-issues:"));
    assert!(shown.findings[0].entry.starts_with("fleet-runs#"));
    db.cleanup().await
}
