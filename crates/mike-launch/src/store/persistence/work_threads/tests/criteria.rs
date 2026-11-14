use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::super::MetaUpdate;
use super::{Fixture, fixture, input};

async fn session_in_project(f: &Fixture) -> anyhow::Result<(i64, Uuid)> {
    let session_uuid = Uuid::new_v4();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(session_uuid)
    .bind(f.project)
    .fetch_one(f.handle.pool())
    .await?;
    Ok((id, session_uuid))
}

async fn thread_with_criteria(
    f: &Fixture,
    slug: &str,
    criteria: Option<&str>,
    session: Option<Uuid>,
) -> anyhow::Result<()> {
    let mut checkpoint = input(slug, "state of play");
    checkpoint.session_uuid = session;
    checkpoint.meta = MetaUpdate {
        title: Some("A thread"),
        acceptance_criteria: criteria,
        ..MetaUpdate::default()
    };
    f.handle
        .launch()
        .checkpoint_work_thread(f.project, &checkpoint)
        .await?;
    Ok(())
}

#[tokio::test]
async fn linked_thread_criteria_lists_only_threads_linked_to_the_session() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (session_id, session_uuid) = session_in_project(&f).await?;
    thread_with_criteria(&f, "crit-first", Some("1. first"), Some(session_uuid)).await?;
    thread_with_criteria(&f, "crit-second", Some("1. second"), Some(session_uuid)).await?;
    thread_with_criteria(&f, "crit-bare", None, Some(session_uuid)).await?;
    thread_with_criteria(&f, "crit-elsewhere", Some("1. elsewhere"), None).await?;

    let found = f
        .handle
        .launch()
        .fetch_linked_thread_criteria(session_id)
        .await?;

    let slugs: Vec<&str> = found.iter().map(|t| t.thread_slug.as_str()).collect();
    assert_eq!(slugs, vec!["crit-second", "crit-first"]);
    assert_eq!(found[0].criteria_text, "1. second");
    assert_eq!(found[0].title, "A thread");
    f._db.cleanup().await
}

#[tokio::test]
async fn linked_thread_criteria_never_borrows_a_project_thread() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (session_id, _) = session_in_project(&f).await?;
    thread_with_criteria(&f, "crit-named", Some("1. named"), None).await?;

    let found = f
        .handle
        .launch()
        .fetch_linked_thread_criteria(session_id)
        .await?;

    assert!(
        found.is_empty(),
        "an unlinked project thread must not reach this session"
    );
    f._db.cleanup().await
}
