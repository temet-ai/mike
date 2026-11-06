use uuid::Uuid;

use super::super::MetaUpdate;
use super::super::criteria::{CriteriaSource, ask_mentions_thread};
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
        .checkpoint_work_thread(f.project, &checkpoint)
        .await?;
    Ok(())
}

#[tokio::test]
async fn thread_criteria_linked_session_thread_wins_over_a_mentioned_one() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (session_id, session_uuid) = session_in_project(&f).await?;
    thread_with_criteria(
        &f,
        "crit-linked",
        Some("1. linked criterion"),
        Some(session_uuid),
    )
    .await?;
    thread_with_criteria(&f, "crit-named", Some("1. named criterion"), None).await?;

    let found = f
        .handle
        .fetch_thread_criteria_for_session(session_id, "please continue thread: crit-named")
        .await?;

    let found = found.ok_or_else(|| anyhow::anyhow!("no criteria found"))?;
    assert_eq!(found.thread_slug, "crit-linked");
    assert_eq!(found.criteria_text, "1. linked criterion");
    assert_eq!(found.via, CriteriaSource::LinkedSession);
    f._db.cleanup().await
}

#[tokio::test]
async fn thread_criteria_unlinked_session_falls_back_to_the_mentioned_project_thread()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (session_id, _) = session_in_project(&f).await?;
    thread_with_criteria(&f, "crit-base", Some("1. base criterion"), None).await?;
    thread_with_criteria(&f, "crit-base-two", Some("1. second criterion"), None).await?;
    thread_with_criteria(&f, "crit-bare", None, None).await?;

    let second = f
        .handle
        .fetch_thread_criteria_for_session(session_id, "work on #crit-base-two today")
        .await?
        .ok_or_else(|| anyhow::anyhow!("no criteria found for the longer slug"))?;
    assert_eq!(second.thread_slug, "crit-base-two");
    assert_eq!(second.via, CriteriaSource::MentionedInAsk);

    let base = f
        .handle
        .fetch_thread_criteria_for_session(session_id, "see artifacts/threads/crit-base/worklog.md")
        .await?
        .ok_or_else(|| anyhow::anyhow!("no criteria found for the path mention"))?;
    assert_eq!(base.thread_slug, "crit-base");
    assert_eq!(base.criteria_text, "1. base criterion");

    let bare = f
        .handle
        .fetch_thread_criteria_for_session(session_id, "thread: crit-bare")
        .await?;
    assert_eq!(bare, None);
    f._db.cleanup().await
}

#[tokio::test]
async fn thread_criteria_without_a_link_or_mention_is_none() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (session_id, session_uuid) = session_in_project(&f).await?;
    thread_with_criteria(&f, "crit-linked-bare", None, Some(session_uuid)).await?;
    thread_with_criteria(&f, "crit-elsewhere", Some("1. elsewhere"), None).await?;

    let unrelated = f
        .handle
        .fetch_thread_criteria_for_session(session_id, "fix the flaky test")
        .await?;
    assert_eq!(unrelated, None);
    let empty = f
        .handle
        .fetch_thread_criteria_for_session(session_id, "")
        .await?;
    assert_eq!(empty, None);
    f._db.cleanup().await
}

#[test]
fn ask_mentions_thread_accepts_the_four_forms() {
    assert!(ask_mentions_thread("thread: alpha-one", "alpha-one"));
    assert!(ask_mentions_thread(
        "resume thread alpha-one now",
        "alpha-one"
    ));
    assert!(ask_mentions_thread("see #alpha-one.", "alpha-one"));
    assert!(ask_mentions_thread(
        "read artifacts/threads/alpha-one/worklog.md",
        "alpha-one"
    ));
}

#[test]
fn ask_mentions_thread_rejects_other_slugs_and_bare_words() {
    assert!(!ask_mentions_thread("thread: alpha-one-b", "alpha-one"));
    assert!(!ask_mentions_thread("thread: alpha-one_b", "alpha-one"));
    assert!(!ask_mentions_thread(
        "alpha-one is mentioned bare",
        "alpha-one"
    ));
    assert!(!ask_mentions_thread("", "alpha-one"));
    assert!(!ask_mentions_thread("thread: ", ""));
}

#[test]
fn ask_mentions_thread_finds_a_later_exact_mention_after_a_longer_slug() {
    assert!(ask_mentions_thread(
        "thread: alpha-one-b and later thread alpha-one",
        "alpha-one"
    ));
}
