use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::io::Cursor;
use std::path::Path;

use uuid::Uuid;

use super::open_tests::open_tokens;
use super::proxy_ledger_tests::quiet_config;
use super::{fake, project_dir, runner};
use crate::launch::cli::ledger::Ledger;
use crate::launch::cli::open;
use crate::launch::cli::queue::{LocalQueue, QueuedLaunch};
use crate::launch::cli::queue_replay;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{NoteOrder, SessionNoteFilter, SessionNoteRow};
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::persistence::workspace::RegisterWorkspaceSession;

struct Queued {
    queue: LocalQueue,
    session: Uuid,
    brief: String,
    launch: QueuedLaunch,
}

async fn open_with_a_long_brief_and_no_ledger(home: &Path) -> anyhow::Result<Queued> {
    let project = project_dir(home);
    let brief = format!(
        "Fix the long list of defects.\n{}end.",
        "detail ".repeat(300)
    );
    let brief_file = home.join("brief.md");
    std::fs::write(&brief_file, &brief)?;
    let backend = fake();
    let config = quiet_config();
    let mut runner = runner(&backend, &config, home);
    let queue = LocalQueue::at(home.join("queue"));
    runner.ledger = Ledger::none().with_queue(Some(queue.clone()));
    let tokens = open_tokens(&project, &["--prompt-file", &brief_file.to_string_lossy()]);
    let output = open::execute(&runner, tokens, &mut Cursor::new(""))
        .await
        .map_err(|f| anyhow::anyhow!("{}", f.error))?;
    let session = output
        .session
        .ok_or_else(|| anyhow::anyhow!("the launch has no session"))?;
    let launch = queue
        .launches()
        .into_iter()
        .next()
        .map(|(_, queued)| queued)
        .ok_or_else(|| anyhow::anyhow!("no launch was queued"))?;
    Ok(Queued {
        queue,
        session,
        brief,
        launch,
    })
}

async fn brief_notes(
    persist: &PersistHandle,
    session: Uuid,
) -> anyhow::Result<Vec<SessionNoteRow>> {
    let filter = SessionNoteFilter {
        kind: Some("brief"),
        status: None,
        tag: None,
        q: None,
        order: NoteOrder::OldestFirst,
        limit: 20,
        offset: 0,
    };
    Ok(persist
        .launch()
        .fetch_session_notes(session, filter)
        .await?)
}

#[tokio::test]
async fn a_brief_delivered_by_file_while_mike_is_unreachable_is_queued_with_its_text_and_a_note_uuid()
-> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let queued = open_with_a_long_brief_and_no_ledger(home.path()).await?;
    let launch = &queued.launch.launch;
    assert_eq!(launch.brief_mode.as_deref(), Some("file"));
    let path = launch
        .brief_path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("the file brief has no path"))?;
    assert_eq!(std::fs::read_to_string(path)?, queued.brief);
    assert_eq!(launch.brief_text.as_deref(), Some(queued.brief.as_str()));
    assert_eq!(launch.session_uuid, Some(queued.session));
    assert!(launch.brief_note_uuid.is_some());
    Ok(())
}

#[tokio::test]
async fn a_queued_file_brief_becomes_the_sessions_brief_note_once_the_queue_drains()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let home = tempfile::tempdir()?;
    let queued = open_with_a_long_brief_and_no_ledger(home.path()).await?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let ledger = Ledger::new(Some(persist.clone())).with_queue(Some(queued.queue.clone()));

    let replay = queue_replay::replay(&ledger).await;
    assert!(replay.stuck.is_empty(), "{replay:?}");
    assert!(queued.queue.launches().is_empty());

    persist
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: queued.session,
            project_path: Some("/tmp/queued-brief"),
            cwd: "/tmp/queued-brief",
            worktree: None,
            registered_via: "rest",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    let notes = brief_notes(&persist, queued.session).await?;
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(
        Some(notes[0].note_uuid),
        queued.launch.launch.brief_note_uuid
    );
    assert_eq!(notes[0].body, queued.brief);
    db.cleanup().await?;
    Ok(())
}
