use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use super::{fixture, seeded_session};
use crate::store::persistence::conditions::thread_context::render_thread_context;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::work_threads::{
    HistoryEntry, ThreadDetail, ThreadIdentity, ThreadState,
};

#[tokio::test]
async fn session_presence_reports_live_released_and_unknown_sessions() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let live = seeded_session(&f.handle).await?;
    let released = seeded_session(&f.handle).await?;
    sqlx::query(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, registered_via, lease_expires_at, released_at)
         VALUES ($1, $2, '/tmp/presence-cwd', 'mcp', now() + interval '15 minutes', NULL),
                ($3, $4, '/tmp/presence-cwd', 'mcp', now() + interval '15 minutes', now())",
    )
    .bind(live.row_id)
    .bind(live.project_id)
    .bind(released.row_id)
    .bind(released.project_id)
    .execute(f.handle.pool())
    .await?;

    let fleet = f.handle.fleet();
    let present = fleet
        .condition_session_presence(live.uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("live session not found"))?;
    assert!(present.live);
    assert_eq!(present.cwd.as_deref(), Some("/tmp/presence-cwd"));
    assert_eq!(present.project_id, live.project_id);

    let gone = fleet
        .condition_session_presence(released.uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("released session not found"))?;
    assert!(!gone.live);

    assert!(
        fleet
            .condition_session_presence(Uuid::new_v4())
            .await?
            .is_none()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn thread_context_for_an_unknown_thread_is_none() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = seeded_session(&f.handle).await?;
    let context = f
        .handle
        .fleet()
        .condition_thread_context(session.project_id, "no-such-thread")
        .await?;
    assert!(context.is_none());
    f.db.cleanup().await
}

fn detail(with_state: bool) -> anyhow::Result<ThreadDetail> {
    let now = Utc::now();
    let date = NaiveDate::from_ymd_opt(2026, 11, 17).ok_or_else(|| anyhow::anyhow!("date"))?;
    let entry = |ord: i32, kind: &str, summary: &str, body: &str| HistoryEntry {
        ord,
        kind: kind.to_owned(),
        entry_date: date,
        session_uuid: None,
        commit_sha: None,
        summary: summary.to_owned(),
        body: body.to_owned(),
        data: None,
        created_at: now,
    };
    Ok(ThreadDetail {
        thread: ThreadIdentity {
            id: 1,
            slug: "side-calls".to_owned(),
            title: Some("Side calls".to_owned()),
            goal: None,
            status: None,
            branch: None,
            worktree: None,
            last_session_uuid: None,
            source_path: None,
            parse_error: None,
            created_at: now,
            updated_at: now,
        },
        state: with_state.then(|| ThreadState {
            tldr: "bookkeeping on a cheap model".to_owned(),
            where_we_are: "worker built".to_owned(),
            next_up: "measure".to_owned(),
            blockers: String::new(),
            open_questions: String::new(),
            decisions: String::new(),
            files: vec![],
            updated_at: now,
        }),
        acceptance_criteria: None,
        depends_on: vec![],
        depended_on_by: vec![],
        history: vec![
            entry(3, "finding", "a finding", "finding body"),
            entry(2, "brief", "newest brief", "do the measuring"),
            entry(1, "brief", "older brief", "old body"),
        ],
        history_total: 3,
        history_next_before_ord: None,
        sessions: vec![],
    })
}

#[test]
fn render_thread_context_names_state_and_the_newest_handoff_entry() -> anyhow::Result<()> {
    let text = render_thread_context(&detail(true)?);
    assert!(
        text.starts_with("Work thread `side-calls` Side calls"),
        "{text}"
    );
    assert!(
        text.contains("tldr: bookkeeping on a cheap model"),
        "{text}"
    );
    assert!(text.contains("where we are: worker built"), "{text}");
    assert!(text.contains("next up: measure"), "{text}");
    assert!(
        text.contains("newest brief entry (2026-11-17): newest brief"),
        "{text}"
    );
    assert!(text.contains("do the measuring"), "{text}");
    assert!(!text.contains("old body"), "{text}");
    assert!(!text.contains("finding body"), "{text}");
    Ok(())
}

#[test]
fn render_thread_context_without_state_keeps_the_header_and_entry() -> anyhow::Result<()> {
    let text = render_thread_context(&detail(false)?);
    assert!(!text.contains("tldr"), "{text}");
    assert!(text.contains("do the measuring"), "{text}");
    Ok(())
}
