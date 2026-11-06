use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use serde_json::json;

use super::{db, quick, repo, stop, wait_for, watching, with_path};
use crate::change_record::ChangeKind;
use crate::change_record::watch::WatchConfig;
use crate::store::persistence::change_record::test_seed::{
    ToolUse, failed_tool_result, request, response, session, tool_result, tool_use,
    tool_use_with_input,
};

#[tokio::test]
async fn an_edit_captured_after_the_file_notification_names_the_editor_though_an_observer_runs_a_command()
-> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let editor = session(&pool, db.project_id, Some(&repo.root)).await?;
    let observer = session(&pool, db.project_id, Some(&repo.root)).await?;
    let started = Utc::now() - Duration::seconds(3);
    let observer_request = request(&pool, observer, db.project_id, started).await?;
    response(
        &pool,
        observer_request,
        Utc::now() - Duration::milliseconds(300),
    )
    .await?;
    let reading = tool_use(
        &pool,
        &ToolUse {
            session_id: observer,
            request_id: observer_request,
            name: "Bash",
            file_path: None,
            at: started,
        },
    )
    .await?;
    let editor_request = request(&pool, editor, db.project_id, started).await?;
    response(&pool, editor_request, Utc::now()).await?;
    let config = WatchConfig {
        author_delay: StdDuration::from_secs(1),
        ..quick(true)
    };
    let running = watching(&db, &repo, config).await?;
    repo.write("src/a.txt", "one\ndef cc():\n    return greet(\"cc\")")?;
    let path = repo
        .git
        .absolute("src/a.txt")
        .to_string_lossy()
        .into_owned();
    let call = tool_use_with_input(
        &pool,
        &ToolUse {
            session_id: editor,
            request_id: editor_request,
            name: "Edit",
            file_path: Some(&path),
            at: started,
        },
        &json!({"file_path": path, "old_string": "one", "new_string": "one\ndef cc():\n    return greet(\"cc\")"}),
    )
    .await?;
    tool_result(&pool, call, editor_request, editor, Utc::now()).await?;
    let seen = wait_for(&db, &repo.root, "the edit's author", |e| {
        with_path(e, "src/a.txt")
            .iter()
            .any(|c| c.detail["author"] != "pending")
    })
    .await?;
    let change = with_path(&seen, "src/a.txt")[0];
    assert_eq!(change.kind, ChangeKind::Edit, "{change:?}");
    assert_eq!(change.session_id, Some(editor));
    assert_eq!(change.detail["tool_use_id"], call);
    tool_result(&pool, reading, observer_request, observer, Utc::now()).await?;
    stop(running, db).await
}

#[tokio::test]
async fn a_failed_edit_whose_text_another_sessions_write_put_in_the_file_names_no_author()
-> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let editor = session(&pool, db.project_id, Some(&repo.root)).await?;
    let writer = session(&pool, db.project_id, Some(&repo.root)).await?;
    let started = Utc::now() - Duration::seconds(3);
    let writer_request = request(&pool, writer, db.project_id, started).await?;
    response(
        &pool,
        writer_request,
        Utc::now() - Duration::milliseconds(300),
    )
    .await?;
    let writing = tool_use(
        &pool,
        &ToolUse {
            session_id: writer,
            request_id: writer_request,
            name: "Bash",
            file_path: None,
            at: started,
        },
    )
    .await?;
    let editor_request = request(&pool, editor, db.project_id, started).await?;
    response(&pool, editor_request, Utc::now()).await?;
    let config = WatchConfig {
        author_delay: StdDuration::from_secs(1),
        ..quick(true)
    };
    let running = watching(&db, &repo, config).await?;
    repo.write("src/a.txt", "rewritten\ndef cc():\n")?;
    let path = repo
        .git
        .absolute("src/a.txt")
        .to_string_lossy()
        .into_owned();
    let call = tool_use_with_input(
        &pool,
        &ToolUse {
            session_id: editor,
            request_id: editor_request,
            name: "Edit",
            file_path: Some(&path),
            at: started,
        },
        &json!({"file_path": path, "old_string": "one", "new_string": "def cc():"}),
    )
    .await?;
    failed_tool_result(&pool, call, editor_request, editor, Utc::now()).await?;
    let seen = wait_for(&db, &repo.root, "the change's author", |e| {
        with_path(e, "src/a.txt")
            .iter()
            .any(|c| c.detail["author"] != "pending")
    })
    .await?;
    let change = with_path(&seen, "src/a.txt")[0];
    assert_eq!(change.kind, ChangeKind::ShellChange, "{change:?}");
    assert_eq!(change.session_id, None);
    assert_eq!(change.detail["author"], "unknown");
    let mut expected = vec![editor, writer];
    expected.sort_unstable();
    assert_eq!(change.detail["candidates"], json!(expected));
    tool_result(&pool, writing, writer_request, writer, Utc::now()).await?;
    stop(running, db).await
}
