use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use sqlx::postgres::PgListener;
use uuid::Uuid;

use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

#[derive(Clone, Copy)]
enum Change {
    Hold,
    LiftHold,
    Claim,
    ReleaseClaim,
    DeadLetter,
}

struct Machines {
    db: TestDbHandle,
    here: PersistHandle,
    other: PersistHandle,
    feed: ChangeFeed,
    session: Uuid,
}

async fn machines() -> anyhow::Result<Option<Machines>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let here = PersistHandle::connect(&db.database_url).await?;
    let other = PersistHandle::connect(&db.database_url).await?;
    let feed = ChangeFeed::new();
    here.attach_change_feed(feed.clone());
    let session = Uuid::new_v4();
    here.launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    Ok(Some(Machines {
        db,
        here,
        other,
        feed,
        session,
    }))
}

impl Machines {
    async fn mail_from_other_machine(&self) -> anyhow::Result<Uuid> {
        let message = Uuid::new_v4();
        self.other
            .launch()
            .insert_message(message, self.session, None, "hello")
            .await?
            .ok_or_else(|| anyhow::anyhow!("mail was not queued"))?;
        Ok(message)
    }

    async fn claimed(&self, honour_held: bool) -> anyhow::Result<usize> {
        Ok(self
            .here
            .fleet()
            .claim_channel_messages_honouring(self.session, 10, honour_held)
            .await?
            .len())
    }

    fn mailbox_moved(&self) {
        self.feed.record(
            RowChangedTable::MailboxMessages,
            Some(&self.session.to_string()),
        );
    }

    async fn set(&self, message: Uuid, change: Change) -> anyhow::Result<()> {
        let sql = match change {
            Change::Hold => {
                "UPDATE mailbox.messages SET held_until_turn = true WHERE message_uuid = $1"
            }
            Change::LiftHold => {
                "UPDATE mailbox.messages SET held_until_turn = false WHERE message_uuid = $1"
            }
            Change::Claim => {
                "UPDATE mailbox.messages SET claimed_at = now() WHERE message_uuid = $1"
            }
            Change::ReleaseClaim => {
                "UPDATE mailbox.messages SET claimed_at = NULL WHERE message_uuid = $1"
            }
            Change::DeadLetter => {
                "UPDATE mailbox.messages SET dead_lettered_at = now() WHERE message_uuid = $1"
            }
        };
        sqlx::query(sql)
            .bind(message)
            .execute(self.other.pool())
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn channel_claim_after_an_empty_claim_answers_from_memory_until_the_mailbox_moves()
-> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    assert_eq!(m.claimed(false).await?, 0);
    m.mail_from_other_machine().await?;
    assert_eq!(
        m.claimed(false).await?,
        0,
        "the other machine's mail has not reached the feed"
    );
    m.mailbox_moved();
    assert_eq!(m.claimed(false).await?, 1);
    m.db.cleanup().await
}

#[tokio::test]
async fn channel_claim_found_empty_while_honouring_holds_still_claims_held_mail_without_them()
-> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let message = m.mail_from_other_machine().await?;
    m.set(message, Change::Hold).await?;
    m.mailbox_moved();
    assert_eq!(m.claimed(true).await?, 0);
    assert_eq!(m.claimed(false).await?, 1);
    m.db.cleanup().await
}

#[tokio::test]
async fn revive_after_finding_nothing_skips_until_the_mailbox_moves() -> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let message = m.mail_from_other_machine().await?;
    let revive = || m.here.launch().revive_undelivered_mail(m.session);
    assert!(revive().await?.is_empty());
    m.set(message, Change::DeadLetter).await?;
    assert!(
        revive().await?.is_empty(),
        "the dead letter has not reached the feed"
    );
    m.mailbox_moved();
    assert_eq!(revive().await?, vec![message]);
    m.db.cleanup().await
}

async fn notifies(m: &Machines, message: Uuid, change: Change) -> anyhow::Result<bool> {
    let mut listener = PgListener::connect(&m.db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    m.set(message, change).await?;
    let wanted = message.to_string();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while let Ok(Ok(note)) = tokio::time::timeout_at(deadline, listener.recv()).await {
        if note.payload().contains(&wanted) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[tokio::test]
async fn mailbox_trigger_lifting_a_hold_or_releasing_a_claim_notifies_but_claiming_does_not()
-> anyhow::Result<()> {
    let Some(m) = machines().await? else {
        return Ok(());
    };
    let message = m.mail_from_other_machine().await?;
    m.set(message, Change::Hold).await?;
    assert!(notifies(&m, message, Change::LiftHold).await?);
    assert!(!notifies(&m, message, Change::Claim).await?);
    assert!(notifies(&m, message, Change::ReleaseClaim).await?);
    m.db.cleanup().await
}
