use uuid::Uuid;

use super::deliver_launch_briefs;
use crate::change_feed::ChangeFeed;
use crate::config::{AppState, ProxyConfig};
use crate::store::models::{ReminderSource, RowChangedTable};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewLaunch;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::workspace::{
    QueuedLaunchBrief, RegisterWorkspaceSession, RegisteredWorkspaceSession,
};

struct Env {
    db: TestDbHandle,
    state: AppState,
    persist: PersistHandle,
}

impl Env {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            return Ok(None);
        };
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            "https://api.anthropic.com",
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:0",
            "--persist-enabled",
            "1",
            "--database-url",
            &db.database_url,
        ])?;
        let state = AppState::new(config).await?;
        let persist = state
            .persist()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("persist_enabled config must yield a PersistHandle"))?;
        Ok(Some(Self { db, state, persist }))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }

    async fn launch(&self, session: Uuid, mode: &str, text: &str) -> anyhow::Result<Uuid> {
        let note = Uuid::new_v4();
        let mut new = NewLaunch::new("herdr", "/tmp/launch-brief");
        new.session_uuid = Some(session);
        new.prompt_present = true;
        new.brief_mode = Some(mode.to_string());
        new.brief_text = Some(text.to_string());
        new.brief_note_uuid = Some(note);
        self.persist.insert_launch(&new).await?;
        Ok(note)
    }

    async fn register(&self, session: Uuid) -> anyhow::Result<RegisteredWorkspaceSession> {
        Ok(self
            .persist
            .register_workspace_session(RegisterWorkspaceSession {
                session_uuid: session,
                project_path: Some("/tmp/launch-brief"),
                cwd: "/tmp/launch-brief",
                worktree: None,
                registered_via: "rest",
                intent: None,
                role: None,
                name: None,
                instructions: None,
                pid: None,
            })
            .await?)
    }

    async fn mail_to(&self, session: Uuid) -> anyhow::Result<Vec<(Uuid, Option<i64>, String)>> {
        Ok(sqlx::query_as(
            "SELECT m.message_uuid, m.from_session_id, m.body
               FROM mailbox.messages m
               JOIN sessions.sessions s ON s.id = m.to_session_id
              WHERE s.session_uuid = $1
              ORDER BY m.id",
        )
        .bind(session)
        .fetch_all(self.persist.pool())
        .await?)
    }

    async fn brief_notes(&self, session: Uuid) -> anyhow::Result<Vec<(Uuid, String)>> {
        Ok(sqlx::query_as(
            "SELECT n.note_uuid, n.body
               FROM workspace.session_notes n
               JOIN sessions.sessions s ON s.id = n.session_id
              WHERE s.session_uuid = $1 AND n.kind = 'brief'
              ORDER BY n.created_at",
        )
        .bind(session)
        .fetch_all(self.persist.pool())
        .await?)
    }
}

async fn mark_delivered(env: &Env, message_uuid: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE mailbox.messages SET sent_at = now(), channel_delivered_at = now() WHERE message_uuid = $1")
        .bind(message_uuid)
        .execute(env.persist.pool())
        .await?;
    Ok(())
}

#[tokio::test]
async fn registration_notes_the_brief_but_does_not_mail_it() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let note = env
        .launch(session, "note", "Fix defects #315 to #322.")
        .await?;
    env.register(session).await?;
    assert!(
        env.mail_to(session).await?.is_empty(),
        "registration must not mail: the launcher pushes once Remote Control is linked"
    );
    assert_eq!(
        env.brief_notes(session).await?,
        vec![(note, "Fix defects #315 to #322.".to_string())]
    );
    env.cleanup().await
}

#[tokio::test]
async fn mailing_a_note_brief_is_idempotent_and_from_mike() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let note = env
        .launch(session, "note", "Fix defects #315 to #322.")
        .await?;
    env.register(session).await?;

    let first = env.persist.mail_launch_briefs(session).await?;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].message_uuid, note);
    let retry = env.persist.mail_launch_briefs(session).await?;
    assert_eq!(
        retry.len(),
        1,
        "an undelivered brief is offered again on retry"
    );
    mark_delivered(&env, note).await?;
    assert!(env.persist.mail_launch_briefs(session).await?.is_empty());

    let mail = env.mail_to(session).await?;
    assert_eq!(mail.len(), 1, "never mailed twice");
    let (uuid, from, body) = &mail[0];
    assert_eq!(*uuid, note);
    assert_eq!(
        *from, None,
        "the brief is mail from mike, not from a session"
    );
    assert!(body.contains("Fix defects #315 to #322."));
    env.cleanup().await
}

#[tokio::test]
async fn inline_and_file_briefs_are_noted_but_never_mailed() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    for mode in ["inline", "file"] {
        let session = Uuid::new_v4();
        let note = env.launch(session, mode, "short brief").await?;
        env.register(session).await?;
        assert!(
            env.persist.mail_launch_briefs(session).await?.is_empty(),
            "{mode}"
        );
        assert!(env.mail_to(session).await?.is_empty(), "{mode}");
        assert_eq!(
            env.brief_notes(session).await?,
            vec![(note, "short brief".to_string())],
            "{mode}"
        );
    }
    env.cleanup().await
}

#[tokio::test]
async fn the_mike_channel_never_claims_brief_mail() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    env.launch(session, "note", "Pushed, never channelled.")
        .await?;
    env.register(session).await?;
    env.persist.mail_launch_briefs(session).await?;
    let claimed = env.persist.claim_channel_messages(session, 10).await?;
    assert!(
        claimed.is_empty(),
        "Claude Code treats channel messages as untrusted, so a brief must not go that way"
    );
    env.cleanup().await
}

#[tokio::test]
async fn an_unpushable_brief_waits_in_the_queue_for_the_next_turn() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let note = env.launch(session, "note", "Queued brief.").await?;
    env.register(session).await?;
    let briefs = env.persist.mail_launch_briefs(session).await?;
    let pushed = deliver_launch_briefs(&env.state, session, &briefs).await;
    assert_eq!(pushed, 0, "no Remote Control stream in the test");

    let pending = env
        .state
        .store
        .read()
        .await
        .peek_reminders(&session.to_string());
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].source,
        ReminderSource::SessionMailbox {
            message_uuid: note,
            from_session_uuid: None,
        }
    );
    assert!(pending[0].payload.contains("Queued brief."));
    env.cleanup().await
}

#[tokio::test]
async fn acknowledging_the_brief_mail_marks_the_launch_as_read() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let note = env.launch(session, "note", "Read me.").await?;
    env.register(session).await?;
    env.persist.mail_launch_briefs(session).await?;
    let unread = env.persist.find_launch_by_session(session).await?;
    assert_eq!(unread.and_then(|r| r.brief_read_at), None);

    mark_delivered(&env, note).await?;
    env.persist.ack_message(session, note).await?;

    let read = env.persist.find_launch_by_session(session).await?;
    assert!(read.and_then(|r| r.brief_read_at).is_some());
    env.cleanup().await
}

fn uuids(briefs: &[QueuedLaunchBrief]) -> Vec<Uuid> {
    briefs.iter().map(|b| b.message_uuid).collect()
}

async fn launch_elsewhere(env: &Env, session: Uuid, text: &str) -> anyhow::Result<Uuid> {
    let other_machine = PersistHandle::connect(&env.db.database_url).await?;
    let note = Uuid::new_v4();
    let mut new = NewLaunch::new("herdr", "/tmp/launch-brief");
    new.session_uuid = Some(session);
    new.prompt_present = true;
    new.brief_mode = Some("note".to_string());
    new.brief_text = Some(text.to_string());
    new.brief_note_uuid = Some(note);
    other_machine.insert_launch(&new).await?;
    Ok(note)
}

async fn settled_session(env: &Env) -> anyhow::Result<(Uuid, Uuid, ChangeFeed)> {
    let feed = ChangeFeed::new();
    env.persist.attach_change_feed(feed.clone());
    let session = Uuid::new_v4();
    let first = env.launch(session, "note", "First brief.").await?;
    env.register(session).await?;
    env.persist.session_row_id(session).await?;
    Ok((session, first, feed))
}

#[tokio::test]
async fn claimable_briefs_once_settled_are_answered_from_memory_until_a_launch_reaches_the_feed()
-> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let (session, first, feed) = settled_session(&env).await?;
    assert_eq!(
        uuids(&env.persist.claimable_launch_briefs(session).await?),
        vec![first]
    );
    assert_eq!(
        uuids(&env.persist.claimable_launch_briefs(session).await?),
        vec![first],
        "an undelivered brief is offered on every claim"
    );
    mark_delivered(&env, first).await?;
    assert!(
        env.persist
            .claimable_launch_briefs(session)
            .await?
            .is_empty()
    );

    let second = launch_elsewhere(&env, session, "Second brief.").await?;
    assert!(
        env.persist
            .claimable_launch_briefs(session)
            .await?
            .is_empty(),
        "nothing on the feed yet, so the settled answer comes from memory"
    );
    feed.record(RowChangedTable::LaunchLaunches, Some(&session.to_string()));
    assert_eq!(
        uuids(&env.persist.claimable_launch_briefs(session).await?),
        vec![second]
    );
    mark_delivered(&env, second).await?;
    feed.record(RowChangedTable::MailboxMessages, Some(&session.to_string()));
    assert!(
        env.persist
            .claimable_launch_briefs(session)
            .await?
            .is_empty()
    );

    let mail = env.mail_to(session).await?;
    assert_eq!(
        mail.iter().map(|(uuid, _, _)| *uuid).collect::<Vec<_>>(),
        vec![first, second],
        "each brief is mailed exactly once"
    );
    env.cleanup().await
}

#[tokio::test]
async fn claimable_briefs_after_a_released_claim_offer_the_brief_again() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    let (session, first, _feed) = settled_session(&env).await?;
    assert_eq!(
        uuids(&env.persist.claimable_launch_briefs(session).await?),
        vec![first]
    );
    assert!(env.persist.claim_message(first).await?);
    assert!(
        env.persist
            .claimable_launch_briefs(session)
            .await?
            .is_empty()
    );
    assert!(env.persist.unclaim_message(first).await?);
    assert_eq!(
        uuids(&env.persist.claimable_launch_briefs(session).await?),
        vec![first],
        "a released claim must reach the next claim as a launch brief"
    );
    env.cleanup().await
}

#[tokio::test]
async fn claimable_briefs_before_the_session_row_is_known_are_never_settled() -> anyhow::Result<()>
{
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };
    env.persist.attach_change_feed(ChangeFeed::new());
    let session = Uuid::new_v4();
    let note = env.launch(session, "note", "Early brief.").await?;
    assert!(
        env.persist
            .claimable_launch_briefs(session)
            .await?
            .is_empty(),
        "no session row yet, so nothing can be mailed"
    );
    env.register(session).await?;
    assert_eq!(
        uuids(&env.persist.claimable_launch_briefs(session).await?),
        vec![note]
    );
    env.cleanup().await
}
