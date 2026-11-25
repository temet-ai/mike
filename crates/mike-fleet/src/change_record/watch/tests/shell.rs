use chrono::{Duration, Utc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{db, quick, repo};
use crate::change_feed::ChangeFeed;
use crate::change_record::ShellCommand;
use crate::change_record::watch::shell_folders::{ShellFolders, folders_named};
use crate::change_record::watch::supervisor::{Supervisor, still_there};
use crate::store::persistence::change_record::test_seed::{request, session};

#[test]
fn folders_named_are_those_a_command_enters_absolute_or_under_the_session_folder() {
    let cases = [
        ("cd /w/b && sed -i s/a/b/ x", vec!["/w/b"]),
        (
            "cd .claude/worktrees/fix\npython3 - <<E",
            vec!["/w/a/.claude/worktrees/fix"],
        ),
        ("cd '/w/with space' && ls", vec!["/w/with space"]),
        ("git -C \"/w/c\" status; cd ../d/e", vec!["/w/c", "/w/d/e"]),
        ("W=/w/f; cd $W", vec!["/w/f"]),
        ("cd /w/a && cd $X && cd ~/y && cd sub && cat /w/z/f", vec![]),
    ];
    for (command, expected) in cases {
        assert_eq!(folders_named(command, "/w/a"), expected, "{command}");
    }
}

fn command(text: &str, at: chrono::DateTime<Utc>) -> ShellCommand {
    ShellCommand {
        id: 900,
        command: text.to_owned(),
        cwd: "/w/a".to_owned(),
        project_id: 1,
        session: Uuid::new_v4(),
        at,
    }
}

#[test]
fn shell_folders_join_once_and_leave_after_thirty_minutes() {
    let mut shell = ShellFolders::default();
    let now = Utc::now();
    let enter = [command("cd /w/b && ls", now)];
    assert_eq!(shell.since(now), (now - Duration::minutes(30), 0));
    assert!(shell.add(&enter, now));
    assert!(!shell.add(&enter, now));
    assert_eq!(shell.since(now), (now - Duration::minutes(30), 700));
    assert!(!shell.wait().is_zero());
    assert_eq!(shell.rows().len(), 1);
    shell.add(&[], now + Duration::minutes(31));
    assert!(shell.rows().is_empty());
}

#[tokio::test]
async fn a_worktree_a_session_entered_in_its_shell_is_watched() -> anyhow::Result<()> {
    let home = repo().await?;
    let other = repo().await?;
    let Some(db) = db(&home.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let worker = session(&pool, db.project_id, Some(&home.root)).await?;
    let req = request(&pool, worker, db.project_id, Utc::now()).await?;
    let shutdown = CancellationToken::new();
    let mut supervisor = Supervisor::new(db.persist.clone(), ChangeFeed::new(), quick(true));
    supervisor.tick(&shutdown).await;
    assert_eq!(supervisor.watching().len(), 1);

    other.write("src/a.txt", "before the watch")?;
    other.write("src/b.txt", "committed before the watch")?;
    other.git(&["add", "src/b.txt"]).await?;
    other.git(&["commit", "-q", "-m", "quick"]).await?;
    sqlx::query(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ('shell-1', $1, 0, 'Bash', jsonb_build_object('command', $2::text), $3)",
    )
    .bind(req)
    .bind(format!("cd {} && python3 edit.py", other.root))
    .bind(worker)
    .execute(&pool)
    .await?;
    supervisor.read_shell(&shutdown).await;
    assert!(supervisor.watching().contains(&other.root));
    let mut recorded = Vec::new();
    for _ in 0..100 {
        recorded = sqlx::query_scalar::<_, String>(
            "SELECT paths[1] FROM workspace.change_events WHERE checkout = $1 ORDER BY 1",
        )
        .bind(&other.root)
        .fetch_all(&pool)
        .await?;
        if !recorded.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(recorded, ["src/a.txt", "src/b.txt"]);

    shutdown.cancel();
    db.db.cleanup().await
}

#[cfg(unix)]
#[tokio::test]
async fn a_root_replaced_by_a_link_to_its_new_place_is_not_there() -> anyhow::Result<()> {
    let moved = repo().await?;
    assert!(still_there(&moved.root));
    let link = format!("{}-old", moved.root);
    std::os::unix::fs::symlink(&moved.root, &link)?;
    let held = still_there(&link);
    std::fs::remove_file(&link)?;
    assert!(!held);
    Ok(())
}
