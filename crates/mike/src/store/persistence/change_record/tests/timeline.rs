use chrono::Duration;

use super::fixture;
use super::seed::{CHECKOUT, event, micros_now, project, request, session};
use crate::change_record::ChangeKind;

#[tokio::test]
async fn open_changes_lists_paths_changed_since_their_newest_commit() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let author = session(pool, project_id, None).await?;
    let mut edit_a = event(project_id, ChangeKind::Edit, &["a.rs"]);
    edit_a.session_id = Some(author);
    let events = vec![
        edit_a,
        event(project_id, ChangeKind::Edit, &["b.rs"]),
        event(project_id, ChangeKind::Commit, &["a.rs", "b.rs"]),
        event(project_id, ChangeKind::ShellChange, &["b.rs", "c.rs"]),
        event(project_id, ChangeKind::Note, &["d.rs"]),
    ];
    f.handle.insert_change_events(&events).await?;

    let open = f.handle.open_changes(CHECKOUT).await?;
    let names: Vec<&str> = open.iter().map(|o| o.path.as_str()).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"b.rs") && names.contains(&"c.rs"));
    assert!(open.iter().all(|o| o.session_id.is_none()));
    assert!(f.handle.open_changes("/work/none").await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn open_changes_reports_the_session_of_the_newest_change_to_a_path() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let first = session(pool, project_id, None).await?;
    let second = session(pool, project_id, None).await?;
    let mut older = event(project_id, ChangeKind::Edit, &["a.rs"]);
    older.session_id = Some(first);
    let mut newer = event(project_id, ChangeKind::Edit, &["a.rs"]);
    newer.session_id = Some(second);
    f.handle.insert_change_events(&[older, newer]).await?;
    let open = f.handle.open_changes(CHECKOUT).await?;
    let [row] = open.as_slice() else {
        anyhow::bail!("expected one open path, got {open:?}");
    };
    assert_eq!(row.session_id, Some(second));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn open_changes_after_a_cached_read_sees_new_events_and_author_updates() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let author = session(pool, project_id, None).await?;
    let ids = f
        .handle
        .insert_change_events(&[event(project_id, ChangeKind::Edit, &["a.rs"])])
        .await?;
    let first = f.handle.open_changes(CHECKOUT).await?;
    assert_eq!(first, f.handle.open_changes(CHECKOUT).await?);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].session_id, None);

    f.handle
        .update_change_author(
            ids[0],
            Some(author),
            ChangeKind::Edit,
            &serde_json::json!({}),
        )
        .await?;
    let named = f.handle.open_changes(CHECKOUT).await?;
    assert_eq!(named.len(), 1);
    assert_eq!(named[0].session_id, Some(author));

    f.handle
        .insert_change_events(&[event(project_id, ChangeKind::Commit, &["a.rs"])])
        .await?;
    assert!(f.handle.open_changes(CHECKOUT).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn checkouts_with_working_sessions_returns_each_folder_with_its_newest_request()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let now = micros_now()?;
    let project_id = project(pool).await?;
    let busy = session(pool, project_id, Some("/work/alpha")).await?;
    let idle = session(pool, project_id, Some("/work/idle")).await?;
    let unplaced = session(pool, project_id, None).await?;
    request(pool, busy, project_id, now - Duration::minutes(20)).await?;
    request(pool, busy, project_id, now - Duration::minutes(2)).await?;
    request(pool, idle, project_id, now - Duration::hours(3)).await?;
    request(pool, unplaced, project_id, now - Duration::minutes(1)).await?;

    let working = f
        .handle
        .checkouts_with_working_sessions(Duration::minutes(30))
        .await?;
    let [one] = working.as_slice() else {
        anyhow::bail!("expected one working checkout, got {working:?}");
    };
    assert_eq!(one.checkout, "/work/alpha");
    assert_eq!(one.project_id, project_id);
    assert_eq!(one.newest_request_at, now - Duration::minutes(2));
    f.db.cleanup().await?;
    Ok(())
}
