use crate::store::persistence::fleet_repo::FleetPersist as _;
use chrono::{Duration, Utc};
use sqlx::PgPool;

use super::{Db, WatchConfig, db, quick, repo, stop, wait_for, watching, with_path};
use crate::change_record::{ChangeEvent, ChangeKind, ChangeQuery};
use crate::store::persistence::change_record::test_seed::{
    ToolUse, request, response, session, tool_use, tool_use_with_input,
};

async fn tool_call(
    pool: &PgPool,
    db: &Db,
    session_id: i64,
    name: &str,
    file_path: Option<&str>,
) -> anyhow::Result<i64> {
    let at = Utc::now() - Duration::seconds(2);
    let request_id = request(pool, session_id, db.project_id, at).await?;
    response(pool, request_id, at).await?;
    tool_use(
        pool,
        &ToolUse {
            session_id,
            request_id,
            name,
            file_path,
            at,
        },
    )
    .await
}

fn named(events: &[ChangeEvent], path: &str) -> bool {
    with_path(events, path)
        .iter()
        .any(|e| e.detail["author"] != "pending")
}

#[tokio::test]
async fn an_edit_and_a_shell_change_are_each_recorded_once_with_their_session() -> anyhow::Result<()>
{
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let editor = session(&pool, db.project_id, Some(&repo.root)).await?;
    let shell = session(&pool, db.project_id, Some(&repo.root)).await?;
    let reader = session(&pool, db.project_id, Some(&repo.root)).await?;
    let edited = repo
        .git
        .absolute("src/a.txt")
        .to_string_lossy()
        .into_owned();
    let call = tool_call(&pool, &db, editor, "Edit", Some(&edited)).await?;
    let before = repo.git(&["rev-parse", "HEAD:src/a.txt"]).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/a.txt", "edited")?;
    wait_for(&db, &repo.root, "the edit's author", |e| {
        named(e, "src/a.txt")
    })
    .await?;
    tool_call(&pool, &db, shell, "Bash", None).await?;
    repo.write("src/from-shell.txt", "made by a command")?;
    let seen = wait_for(&db, &repo.root, "both authors", |e| {
        named(e, "src/a.txt") && named(e, "src/from-shell.txt")
    })
    .await?;
    let edit = with_path(&seen, "src/a.txt");
    assert_eq!(edit.len(), 1, "{seen:?}");
    assert_eq!(edit[0].kind, ChangeKind::Edit);
    assert_eq!(edit[0].session_id, Some(editor));
    assert_eq!(edit[0].detail["tool_use_id"], call);
    assert_eq!(edit[0].before_sha.as_deref(), Some(before.as_str()));
    let after = edit[0].after_sha.clone().unwrap_or_default();
    assert_eq!(repo.git(&["cat-file", "-p", &after]).await?, "edited");
    let by_shell = with_path(&seen, "src/from-shell.txt");
    assert_eq!(by_shell.len(), 1, "{seen:?}");
    assert_eq!(by_shell[0].kind, ChangeKind::ShellChange);
    assert_eq!(by_shell[0].session_id, Some(shell));

    let place = db.persist.fleet().change_place(reader, &repo.root).await?;
    let mut asked = ChangeQuery::everything_after(repo.root.clone(), place, 100);
    asked.exclude_session = Some(reader);
    let first = db.persist.fleet().change_events_after(&asked).await?;
    assert_eq!(
        first.len(),
        2,
        "the reader is shown both rows once: {first:?}"
    );
    let last = first.iter().map(|e| e.id).max().unwrap_or(place);
    db.persist
        .fleet()
        .move_change_place(reader, &repo.root, last)
        .await?;
    asked.after_id = db.persist.fleet().change_place(reader, &repo.root).await?;
    assert!(
        db.persist
            .fleet()
            .change_events_after(&asked)
            .await?
            .is_empty()
    );
    stop(running, db).await
}

#[tokio::test]
async fn two_overlapping_shell_commands_record_no_author() -> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let one = session(&pool, db.project_id, Some(&repo.root)).await?;
    let two = session(&pool, db.project_id, Some(&repo.root)).await?;
    tool_call(&pool, &db, one, "Bash", None).await?;
    tool_call(&pool, &db, two, "Bash", None).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/whose.txt", "nobody can tell")?;
    let seen = wait_for(&db, &repo.root, "the unnamed change", |e| {
        named(e, "src/whose.txt")
    })
    .await?;
    let change = with_path(&seen, "src/whose.txt")[0];
    assert_eq!(change.session_id, None);
    assert_eq!(change.kind, ChangeKind::ShellChange);
    assert_eq!(change.detail["author"], "unknown");
    let mut expected = vec![one, two];
    expected.sort_unstable();
    assert_eq!(change.detail["candidates"], serde_json::json!(expected));
    stop(running, db).await
}

async fn unnamed_after(
    db: &Db,
    repo: &super::Repo,
    path: &str,
    expected: &mut Vec<i64>,
) -> anyhow::Result<()> {
    let seen = wait_for(db, &repo.root, "the unnamed edit", |e| named(e, path)).await?;
    let change = with_path(&seen, path)[0];
    assert_eq!(change.session_id, None, "{change:?}");
    assert_eq!(change.kind, ChangeKind::ShellChange);
    assert_eq!(change.detail["author"], "unknown");
    expected.sort_unstable();
    assert_eq!(change.detail["candidates"], serde_json::json!(expected));
    Ok(())
}

#[tokio::test]
async fn an_edit_while_another_sessions_shell_runs_records_no_author() -> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let editor = session(&pool, db.project_id, Some(&repo.root)).await?;
    let formatter = session(&pool, db.project_id, Some(&repo.root)).await?;
    let path = repo
        .git
        .absolute("src/a.txt")
        .to_string_lossy()
        .into_owned();
    tool_call(&pool, &db, editor, "Edit", Some(&path)).await?;
    tool_call(&pool, &db, formatter, "Bash", None).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/a.txt", "edited, or formatted?")?;
    unnamed_after(&db, &repo, "src/a.txt", &mut vec![editor, formatter]).await?;
    stop(running, db).await
}

#[tokio::test]
async fn two_sessions_editing_the_same_file_record_no_author() -> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let first = session(&pool, db.project_id, Some(&repo.root)).await?;
    let second = session(&pool, db.project_id, Some(&repo.root)).await?;
    let path = repo
        .git
        .absolute("src/a.txt")
        .to_string_lossy()
        .into_owned();
    tool_call(&pool, &db, first, "Edit", Some(&path)).await?;
    tool_call(&pool, &db, second, "Write", Some(&path)).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/a.txt", "whose edit is this?")?;
    unnamed_after(&db, &repo, "src/a.txt", &mut vec![first, second]).await?;
    stop(running, db).await
}

async fn edit_call(
    pool: &PgPool,
    db: &Db,
    session_id: i64,
    path: &str,
    old: &str,
    new: &str,
) -> anyhow::Result<i64> {
    let at = Utc::now() - Duration::seconds(2);
    let request_id = request(pool, session_id, db.project_id, at).await?;
    response(pool, request_id, at).await?;
    tool_use_with_input(
        pool,
        &ToolUse {
            session_id,
            request_id,
            name: "Edit",
            file_path: Some(path),
            at,
        },
        &serde_json::json!({"file_path": path, "old_string": old, "new_string": new}),
    )
    .await
}

#[tokio::test]
async fn an_edit_proven_by_its_text_stays_proven_when_it_is_the_only_change_made_beside_a_shell()
-> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let editor = session(&pool, db.project_id, Some(&repo.root)).await?;
    let other = session(&pool, db.project_id, Some(&repo.root)).await?;
    let path = repo
        .git
        .absolute("src/a.txt")
        .to_string_lossy()
        .into_owned();
    let call = edit_call(&pool, &db, editor, &path, "one", "ONE").await?;
    tool_call(&pool, &db, other, "Bash", None).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/a.txt", "ONE")?;
    let seen = wait_for(&db, &repo.root, "the proven edit", |e| {
        named(e, "src/a.txt")
    })
    .await?;
    let change = with_path(&seen, "src/a.txt")[0];
    assert_eq!(change.session_id, Some(editor), "{change:?}");
    assert_eq!(change.detail["tool_use_id"], call);
    stop(running, db).await
}

#[tokio::test]
async fn an_edit_and_another_sessions_shell_write_in_the_same_second_record_no_author()
-> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let editor = session(&pool, db.project_id, Some(&repo.root)).await?;
    let writer = session(&pool, db.project_id, Some(&repo.root)).await?;
    let path = repo
        .git
        .absolute("src/a.txt")
        .to_string_lossy()
        .into_owned();
    edit_call(&pool, &db, editor, &path, "one", "ONE").await?;
    tool_call(&pool, &db, writer, "Bash", None).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/a.txt", "ONE\nappended by a command")?;
    unnamed_after(&db, &repo, "src/a.txt", &mut vec![editor, writer]).await?;
    stop(running, db).await
}

#[tokio::test]
async fn an_edit_whose_row_is_stored_after_the_file_was_first_named_is_named_on_the_second_pass()
-> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let editor = session(&pool, db.project_id, Some(&repo.root)).await?;
    let path = repo
        .git
        .absolute("src/a.txt")
        .to_string_lossy()
        .into_owned();
    let running = watching(
        &db,
        &repo,
        WatchConfig {
            hold_retries: true,
            ..quick(true)
        },
    )
    .await?;
    repo.write("src/a.txt", "ONE")?;
    wait_for(&db, &repo.root, "the first naming", |e| {
        named(e, "src/a.txt")
    })
    .await?;
    let at = Utc::now() - Duration::milliseconds(500);
    let request_id = request(&pool, editor, db.project_id, at).await?;
    response(&pool, request_id, at).await?;
    let late = tool_use_with_input(
        &pool,
        &ToolUse {
            session_id: editor,
            request_id,
            name: "Edit",
            file_path: Some(&path),
            at: Utc::now(),
        },
        &serde_json::json!({"file_path": path, "old_string": "one", "new_string": "ONE"}),
    )
    .await?;
    running
        .stats
        .retries_held
        .store(false, std::sync::atomic::Ordering::Release);
    let seen = wait_for(&db, &repo.root, "the second naming", |e| {
        with_path(e, "src/a.txt")
            .iter()
            .any(|c| c.detail["author"] == "tool_call")
    })
    .await?;
    let change = with_path(&seen, "src/a.txt")[0];
    assert_eq!(change.session_id, Some(editor), "{change:?}");
    assert_eq!(change.detail["tool_use_id"], late);
    stop(running, db).await
}
