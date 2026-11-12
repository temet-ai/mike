use uuid::Uuid;

use super::Stage;
use super::claude::ClaudeRecipient;
use super::codex::CodexRecipient;
use super::deliver::{Outcome, deliver};
use super::mailbox_link::{mailbox_state, mirror_codex_mail};
use super::scripted::{Script, ScriptedSeat, as_recipient, database, queued};
use super::store;
use super::{Mode, Runtime};
use crate::launch::codex::bind_seat_thread;

#[tokio::test]
async fn mail_outbox_restart_with_the_message_in_the_resumed_thread_does_not_resend()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        ..Script::default()
    });
    let message = queued(&persist, 1).await?;
    let accepted = deliver(pool, as_recipient(&seat.recipient()), message).await?;
    assert!(matches!(accepted, Outcome::Accepted { .. }), "{accepted:?}");

    let restarted = ScriptedSeat::with(Script {
        generation: 2,
        ..Script::default()
    });
    let turns = seat.read(|s| s.turns.clone()).unwrap_or_default();
    restarted.edit(|s| s.turns = turns);
    let recovered = deliver(pool, as_recipient(&restarted.recipient()), message).await?;
    assert!(matches!(recovered, Outcome::Delivered(_)), "{recovered:?}");
    assert_eq!(
        restarted.read(|s| s.calls.iter().filter(|c| c.starts_with("turn/")).count()),
        Some(0)
    );
    assert_eq!(store::attempts(pool, message).await?.len(), 1);
    assert_eq!(
        store::load(pool, message).await?.map(|m| m.generation),
        Some(2)
    );
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_restart_that_lost_an_accepted_message_sends_it_once_more() -> anyhow::Result<()>
{
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let message = queued(&persist, 1).await?;
    let old = store::begin_attempt(pool, message, 1, &Mode::Start).await?;
    store::finish_attempt(pool, old, "accepted", Some("turn-lost"), None).await?;

    let restarted = ScriptedSeat::with(Script {
        generation: 2,
        ..Script::default()
    });
    let recipient = restarted.recipient();
    let resent = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(resent, Outcome::Accepted { .. }), "{resent:?}");
    let again = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(again, Outcome::Delivered(_)), "{again:?}");
    assert_eq!(restarted.copies_of(message), 1);
    let generations: Vec<_> = store::attempts(pool, message)
        .await?
        .iter()
        .map(|a| a.generation)
        .collect();
    assert_eq!(generations, vec![1, 2]);
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_mail_for_a_stopped_seat_waits_and_is_delivered_when_it_returns()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let seat = ScriptedSeat::with(Script {
        generation: 1,
        dead: true,
        ..Script::default()
    });
    let recipient = seat.recipient();
    let message = queued(&persist, 1).await?;
    let waiting = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(waiting, Outcome::Waiting(_)), "{waiting:?}");
    assert!(store::attempts(pool, message).await?.is_empty());
    assert!(store::open_messages(pool, 10_000).await?.contains(&message));

    seat.edit(|s| {
        s.dead = false;
        s.generation = 3;
    });
    deliver(pool, as_recipient(&recipient), message).await?;
    let delivered = deliver(pool, as_recipient(&recipient), message).await?;
    assert!(matches!(delivered, Outcome::Delivered(_)), "{delivered:?}");
    assert_eq!(seat.copies_of(message), 1);
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_mirrors_claude_to_codex_and_codex_to_claude_mail_once() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let pool = persist.pool();
    let (claude, codex) = (Uuid::new_v4(), Uuid::new_v4());
    persist.ensure_session_exists(&claude.to_string()).await?;
    let home = std::env::temp_dir().join(format!("mail-outbox-{codex}"));
    bind_seat_thread(&persist, &home, codex, "thread-1", None).await?;
    let (to_codex, to_claude) = (Uuid::new_v4(), Uuid::new_v4());
    persist
        .insert_message(to_codex, codex, Some(claude), "from claude")
        .await?;
    persist
        .insert_message(to_claude, claude, Some(codex), "from codex")
        .await?;

    assert_eq!(mirror_codex_mail(pool, 1_000).await?, 2);
    assert_eq!(mirror_codex_mail(pool, 1_000).await?, 0);
    let runtime = |m: Option<store::Mail>| m.and_then(|m| m.runtime);
    assert_eq!(
        runtime(store::load(pool, to_codex).await?),
        Some(Runtime::Codex)
    );
    assert_eq!(
        runtime(store::load(pool, to_claude).await?),
        Some(Runtime::Claude)
    );
    let from = store::load(pool, to_codex).await?.and_then(|m| m.from);
    assert_eq!(from, Some(claude));

    let seat = ScriptedSeat::with(Script {
        generation: 0,
        ..Script::default()
    });
    let busy_turn = seat.busy();
    let recipient = CodexRecipient {
        pool: Some(pool.clone()),
        ..seat.recipient()
    };
    let steered = deliver(pool, as_recipient(&recipient), to_codex).await?;
    assert_eq!(steered, Outcome::Accepted { turn_id: busy_turn });
    deliver(pool, as_recipient(&recipient), to_codex).await?;
    let state = mailbox_state(pool, to_codex)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no row"))?;
    assert!(
        state.claimed_at.is_some() && state.consumed_at.is_some(),
        "{state:?}"
    );

    let claude_side = ClaudeRecipient {
        persist: persist.clone(),
        session: claude,
    };
    let queued_only = deliver(pool, &claude_side, to_claude).await?;
    assert!(
        matches!(queued_only, Outcome::Waiting(_)),
        "{queued_only:?}"
    );
    sqlx::query("UPDATE mailbox.messages SET channel_delivered_at = now() WHERE message_uuid = $1")
        .bind(to_claude)
        .execute(pool)
        .await?;
    let pushed = deliver(pool, &claude_side, to_claude).await?;
    assert!(matches!(pushed, Outcome::Accepted { .. }), "{pushed:?}");
    sqlx::query("UPDATE mailbox.messages SET consumed_at = now() WHERE message_uuid = $1")
        .bind(to_claude)
        .execute(pool)
        .await?;
    let consumed = deliver(pool, &claude_side, to_claude).await?;
    assert!(matches!(consumed, Outcome::Delivered(_)), "{consumed:?}");
    let stages: Vec<_> = store::stages(pool, to_claude)
        .await?
        .into_iter()
        .map(|(s, _, _)| s)
        .collect();
    assert_eq!(
        stages,
        vec![Stage::Queued, Stage::Accepted, Stage::Consumed]
    );
    db.cleanup().await
}

#[tokio::test]
async fn mail_outbox_unrelated_claude_to_claude_mail_is_not_mirrored() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    persist.ensure_session_exists(&a.to_string()).await?;
    persist.ensure_session_exists(&b.to_string()).await?;
    let message = Uuid::new_v4();
    persist
        .insert_message(message, b, Some(a), "claude to claude")
        .await?;
    mirror_codex_mail(persist.pool(), 1_000).await?;
    assert!(store::load(persist.pool(), message).await?.is_none());
    db.cleanup().await
}
