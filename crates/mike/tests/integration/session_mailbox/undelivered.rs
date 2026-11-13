use crate::seed::{seed_project, seed_session};
use crate::test_db::TestDb;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

struct Pair {
    handle: PersistHandle,
    sender: Uuid,
    recipient: Uuid,
}

async fn pair(
    factory: &TestDb,
    name: &str,
) -> anyhow::Result<(Pair, crate::test_db::TestDbHandle)> {
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), &format!("/proj/{name}-{}", Uuid::new_v4())).await?;
    let (sender, recipient) = (Uuid::new_v4(), Uuid::new_v4());
    seed_session(handle.pool(), project, &sender.to_string()).await?;
    seed_session(handle.pool(), project, &recipient.to_string()).await?;
    Ok((
        Pair {
            handle,
            sender,
            recipient,
        },
        db,
    ))
}

async fn notices_for(handle: &PersistHandle, session: Uuid) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT m.body FROM mailbox.messages m
           JOIN sessions.sessions s ON s.id = m.to_session_id
          WHERE s.session_uuid = $1 AND m.body LIKE '[mike mail] Not delivered:%'
          ORDER BY m.id",
    )
    .bind(session)
    .fetch_all(handle.pool())
    .await?;
    Ok(rows.into_iter().map(|(b,)| b).collect())
}

#[tokio::test]
async fn a_message_given_up_on_is_told_to_its_sender_once_naming_the_mail_and_the_recipient()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let (p, db) = pair(&factory, "undelivered-notice").await?;
    let mail = Uuid::new_v4();
    p.handle
        .launch()
        .insert_message(mail, p.recipient, Some(p.sender), "please look")
        .await?;

    assert!(p.handle.launch().tell_sender_not_delivered(mail).await?);
    assert!(
        !p.handle.launch().tell_sender_not_delivered(mail).await?,
        "the sender is told once"
    );

    let notices = notices_for(&p.handle, p.sender).await?;
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains(&mail.to_string()), "{}", notices[0]);
    assert!(
        notices[0].contains(&p.recipient.to_string()),
        "{}",
        notices[0]
    );

    p.handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_reaper_tells_the_sender_and_the_receivers_next_request_revives_the_mail()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let (p, db) = pair(&factory, "undelivered-reaped").await?;
    let stranded = Uuid::new_v4();
    p.handle
        .launch()
        .insert_message(stranded, p.recipient, Some(p.sender), "stranded")
        .await?;
    sqlx::query("UPDATE mailbox.messages SET queued_at = now() - interval '2 hours' WHERE message_uuid = $1")
        .bind(stranded)
        .execute(p.handle.pool())
        .await?;
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(60);
    let reaped = p
        .handle
        .launch()
        .dead_letter_expired_undelivered(cutoff, cutoff)
        .await?;
    assert_eq!(reaped.dead_lettered, 1);

    let notices = notices_for(&p.handle, p.sender).await?;
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains(&stranded.to_string()));

    let revived = p
        .handle
        .launch()
        .revive_undelivered_mail(p.recipient)
        .await?;
    assert_eq!(revived, vec![stranded]);
    assert_eq!(
        p.handle
            .fleet()
            .claim_channel_messages(p.recipient, 10)
            .await?
            .len(),
        1
    );

    p.handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mail_the_receiver_saw_and_never_acknowledged_is_not_revived() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let (p, db) = pair(&factory, "undelivered-seen").await?;
    let seen = Uuid::new_v4();
    p.handle
        .launch()
        .insert_message(seen, p.recipient, Some(p.sender), "seen")
        .await?;
    assert_eq!(
        p.handle
            .fleet()
            .claim_channel_messages(p.recipient, 10)
            .await?
            .len(),
        1
    );
    sqlx::query("UPDATE mailbox.messages SET dead_lettered_at = now() WHERE message_uuid = $1")
        .bind(seen)
        .execute(p.handle.pool())
        .await?;

    assert!(
        p.handle
            .launch()
            .revive_undelivered_mail(p.recipient)
            .await?
            .is_empty()
    );

    p.handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn twelve_messages_to_a_busy_seat_end_with_one_notice_each_naming_what_happens_next()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let (p, db) = pair(&factory, "undelivered-burst").await?;
    let mut mails = Vec::new();
    for n in 0..12 {
        let mail = Uuid::new_v4();
        p.handle
            .launch()
            .insert_message(mail, p.recipient, Some(p.sender), &format!("burst {n}"))
            .await?;
        mails.push(mail);
    }
    for mail in &mails {
        assert!(p.handle.launch().claim_message(*mail).await?);
        assert!(
            p.handle
                .launch()
                .stamp_mailbox_sent_without_request(*mail)
                .await?
        );
    }
    for mail in &mails {
        let mut gave_up = false;
        for _ in 0..4 {
            let outcome = p
                .handle
                .launch()
                .bump_redelivery_attempt(*mail, 3, 0, 0)
                .await?;
            gave_up = outcome.is_some_and(|o| o.dead_lettered);
            if gave_up {
                break;
            }
        }
        assert!(gave_up, "the seat never acknowledged {mail}");
        assert!(p.handle.launch().tell_sender_not_delivered(*mail).await?);
    }

    let notices = notices_for(&p.handle, p.sender).await?;
    assert_eq!(notices.len(), 12, "one notice per message: {notices:?}");
    for mail in &mails {
        let own: Vec<&String> = notices
            .iter()
            .filter(|n| n.contains(&mail.to_string()))
            .collect();
        assert_eq!(own.len(), 1, "{mail}");
        assert!(
            own[0].contains("mike will not deliver it again"),
            "{}",
            own[0]
        );
    }
    assert!(
        p.handle
            .launch()
            .revive_undelivered_mail(p.recipient)
            .await?
            .is_empty(),
        "delivered mail is not revived, so the notice's promise matches what happens"
    );

    p.handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
