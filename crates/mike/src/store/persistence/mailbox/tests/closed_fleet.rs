use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::seed_fleet_running;
use serde_json::json;
use uuid::Uuid;

use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::questions::ask_and_mail;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::{NewLaunch, NewRun};
use crate::store::persistence::mailbox::FleetTarget;
use crate::store::persistence::questions::AskedQuestion;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

async fn launch(
    handle: &PersistHandle,
    run: &str,
    session: Uuid,
    seat: &str,
) -> anyhow::Result<i64> {
    handle
        .launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    let mut new = NewLaunch::new("herdr", "/tmp/closed-mail-test");
    new.run_id = Some(run.to_owned());
    new.session_uuid = Some(session);
    new.seat = Some(seat.to_owned());
    Ok(handle.launch().insert_launch(&new).await?)
}

#[tokio::test]
async fn closed_fleet_seat_refuses_raw_mail_and_questions_and_routes_task_to_successor()
-> anyhow::Result<()> {
    let Some((db, handle, run)) = fixture().await? else {
        return Ok(());
    };
    let (sender, old, successor) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    launch(&handle, &run, sender, "coordinator").await?;
    let old_launch = launch(&handle, &run, old, "worker-P1").await?;
    launch(&handle, &run, successor, "worker-P1").await?;
    let def: FleetDefinition = serde_json::from_value(
        json!({"coordinator": {"session": sender.to_string()},
        "tasks": {"P1": {"seat": "worker-P1", "session": successor.to_string(), "files": ["src/owned.rs"]}}}),
    )?;
    let fleet = handle
        .fleet()
        .insert_fleet(&NewFleet {
            slug: run.clone(),
            project_path: "/tmp/closed-mail-test".into(),
            definition: serde_json::to_value(&def)?,
            missing: json!([]),
            created_by: None,
        })
        .await?;
    seed_fleet_running(&handle, fleet.fleet_uuid, &run).await?;
    let pending = Uuid::new_v4();
    assert!(
        handle
            .launch()
            .insert_message(pending, old, Some(sender), "before closure\nFull body")
            .await?
            .is_some()
    );
    handle
        .launch()
        .record_launch_closed(old_launch, "complete", Some(true))
        .await?;
    let closed = handle
        .launch()
        .closed_fleet_seat(old)
        .await?
        .ok_or_else(|| anyhow::anyhow!("closed seat not detected"))?;
    assert_eq!(closed.task.as_deref(), Some("P1"));
    assert_eq!(closed.successor, Some(successor));
    assert!(closed.refusal(old).contains(&successor.to_string()));
    for from in [Some(sender), None] {
        assert!(
            handle
                .launch()
                .insert_message(Uuid::new_v4(), old, from, "must not queue")
                .await?
                .is_none()
        );
        assert_closed_audits(&handle, sender, old, 1).await?;
    }
    assert!(!handle.launch().tell_sender_not_delivered(pending).await?);
    age_message(&handle, pending).await?;
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
    handle
        .launch()
        .dead_letter_expired_undelivered(cutoff, cutoff)
        .await?;
    assert!(
        handle
            .launch()
            .fetch_for_session_with_activity(sender)
            .await?
            .inbox
            .is_empty()
    );
    let asked = ask_and_mail(
        &handle,
        &AskedQuestion {
            asker: sender,
            to_session: Some(old),
            to_task: Some("P1"),
            run_id: Some(&run),
            about_note: None,
            question: "who owns it?",
            blocked: true,
        },
        None,
    )
    .await;
    assert!(asked.is_err_and(|err| err.to_string().contains("closed")));
    assert!(
        handle
            .launch()
            .open_questions_in_run(&run)
            .await?
            .is_empty()
    );
    for address in ["task:P1", "owner:src/owned.rs"] {
        assert_eq!(
            handle
                .launch()
                .resolve_fleet_target(sender, address)
                .await?,
            Some(FleetTarget::Seat {
                session: successor,
                task: Some("P1".into())
            })
        );
    }
    let delivered = handle
        .launch()
        .insert_message(Uuid::new_v4(), successor, Some(sender), "successor")
        .await?;
    assert!(delivered.is_some());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn undelivered_open_seat_notice_names_task_and_first_line_once() -> anyhow::Result<()> {
    let Some((db, handle, run)) = fixture().await? else {
        return Ok(());
    };
    let (sender, recipient) = (Uuid::new_v4(), Uuid::new_v4());
    launch(&handle, &run, sender, "coordinator").await?;
    launch(&handle, &run, recipient, "worker-P1").await?;
    let def: FleetDefinition = serde_json::from_value(
        json!({"tasks": {"P1": {"seat": "worker-P1", "session": recipient.to_string()}}}),
    )?;
    let fleet = handle
        .fleet()
        .insert_fleet(&NewFleet {
            slug: run.clone(),
            project_path: "/tmp/closed-mail-test".into(),
            definition: serde_json::to_value(&def)?,
            missing: json!([]),
            created_by: None,
        })
        .await?;
    seed_fleet_running(&handle, fleet.fleet_uuid, &run).await?;
    let message = Uuid::new_v4();
    handle
        .launch()
        .insert_message(
            message,
            recipient,
            Some(sender),
            "About the journal arm\nFull body remains stored",
        )
        .await?;
    age_message(&handle, message).await?;
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
    assert_eq!(handle.launch().tell_inactive_senders(cutoff).await?, 1);
    assert_eq!(handle.launch().tell_inactive_senders(cutoff).await?, 0);
    handle
        .launch()
        .dead_letter_expired_undelivered(cutoff, cutoff)
        .await?;
    assert!(!handle.launch().tell_sender_not_delivered(message).await?);
    let inbox = handle
        .launch()
        .fetch_for_session_with_activity(sender)
        .await?
        .inbox;
    assert_eq!(inbox.len(), 1);
    for expected in [
        "task P1",
        "worker-P1",
        "About the journal arm",
        &message.to_string(),
    ] {
        assert!(
            inbox[0].row.body.contains(expected),
            "{}",
            inbox[0].row.body
        );
    }
    db.cleanup().await?;
    Ok(())
}

async fn fixture() -> anyhow::Result<Option<(TestDbHandle, PersistHandle, String)>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let run = format!("closed-mail-{}", Uuid::new_v4());
    handle
        .launch()
        .upsert_run(&NewRun {
            run_id: run.clone(),
            project_path: None,
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    Ok(Some((db, handle, run)))
}

#[tokio::test]
async fn a_seat_resumed_after_its_launch_closed_is_open_again() -> anyhow::Result<()> {
    let Some((db, handle, run)) = fixture().await? else {
        return Ok(());
    };
    let (lead, worker, sender) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let lead_launch = launch(&handle, &run, lead, "coordinator").await?;
    let worker_launch = launch(&handle, &run, worker, "worker-P1").await?;
    launch(&handle, &run, sender, "acceptance").await?;
    let def: FleetDefinition = serde_json::from_value(json!({
        "coordinator": {"session": lead.to_string()},
        "tasks": {"P1": {"seat": "worker-P1", "session": worker.to_string(), "files": ["src/a.rs"]}}
    }))?;
    let fleet = handle
        .fleet()
        .insert_fleet(&NewFleet {
            slug: run.clone(),
            project_path: "/tmp/closed-mail-test".into(),
            definition: serde_json::to_value(&def)?,
            missing: json!([]),
            created_by: None,
        })
        .await?;
    seed_fleet_running(&handle, fleet.fleet_uuid, &run).await?;
    for closed in [lead_launch, worker_launch] {
        handle
            .launch()
            .record_launch_closed(closed, "process exited", Some(true))
            .await?;
    }
    let refused = handle.launch().closed_fleet_seat(worker).await?;
    assert_eq!(refused.as_ref().and_then(|seat| seat.coordinator), None);
    assert!(
        handle
            .launch()
            .insert_message(Uuid::new_v4(), lead, Some(sender), "before resume")
            .await?
            .is_none()
    );
    sqlx::query("UPDATE sessions.sessions SET last_seen = now() + interval '1 minute' WHERE session_uuid = $1")
        .bind(lead)
        .execute(handle.pool())
        .await?;
    assert_eq!(handle.launch().closed_fleet_seat(lead).await?, None);
    assert!(
        handle
            .launch()
            .insert_message(Uuid::new_v4(), lead, Some(sender), "after resume")
            .await?
            .is_some()
    );
    let routed = handle.launch().closed_fleet_seat(worker).await?;
    assert_eq!(routed.and_then(|seat| seat.coordinator), Some(lead));
    db.cleanup().await?;
    Ok(())
}

async fn age_message(handle: &PersistHandle, message: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE mailbox.messages SET queued_at = now() - interval '10 minutes' WHERE message_uuid = $1")
        .bind(message).execute(handle.pool()).await?;
    Ok(())
}

async fn assert_closed_audits(
    handle: &PersistHandle,
    sender: Uuid,
    recipient: Uuid,
    count: usize,
) -> anyhow::Result<()> {
    let rows: Vec<(Option<Uuid>, String, String)> = sqlx::query_as(
        "SELECT sender.session_uuid, failed.attempted_to, failed.reason
           FROM mailbox.failed_sends failed
           LEFT JOIN sessions.sessions sender ON sender.id = failed.attempted_by_session_id
          WHERE failed.attempted_to = $1 OR sender.session_uuid = $2",
    )
    .bind(recipient.to_string())
    .bind(sender)
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(
        rows,
        vec![
            (
                Some(sender),
                recipient.to_string(),
                "recipient_ended".to_owned()
            );
            count
        ]
    );
    Ok(())
}

#[tokio::test]
async fn shared_insert_guard_refuses_closed_recipients_with_or_without_context()
-> anyhow::Result<()> {
    let Some((db, handle, run)) = fixture().await? else {
        return Ok(());
    };
    let (sender, recipient) = (Uuid::new_v4(), Uuid::new_v4());
    launch(&handle, &run, sender, "coordinator").await?;
    let old = launch(&handle, &run, recipient, "worker").await?;
    handle
        .launch()
        .record_launch_closed(old, "complete", Some(true))
        .await?;
    let recipient_id = handle
        .launch()
        .resolve_session_core(recipient)
        .await?
        .ok_or_else(|| anyhow::anyhow!("recipient missing"))?
        .0;
    let thread = handle
        .launch()
        .create_thread(Uuid::new_v4(), "guarded thread", None, None)
        .await?;
    let items = vec![crate::store::persistence::mailbox::ContextItem {
        kind: "file".into(),
        value: "src/owned.rs".into(),
        label: None,
    }];
    for (attempt, context) in [&[][..], items.as_slice()].into_iter().enumerate() {
        assert!(
            handle
                .launch()
                .insert_message_with_context(
                    Uuid::new_v4(),
                    recipient,
                    Some(sender),
                    "closed",
                    context
                )
                .await?
                .is_none()
        );
        assert_closed_audits(&handle, sender, recipient, attempt + 1).await?;
        let id = Uuid::new_v4();
        assert!(
            handle
                .launch()
                .insert_threaded_message_with_context(
                    crate::store::persistence::mailbox::ThreadedMessageInsert {
                        message_uuid: id,
                        to_session_id: Some(recipient_id),
                        from_session_id: None,
                        body: "closed threaded",
                        thread_id: Some(thread.id),
                        in_reply_to_id: None,
                    },
                    context,
                )
                .await?
                .is_none()
        );
        assert!(
            handle
                .launch()
                .fetch_message_core_by_uuid(id)
                .await?
                .is_none()
        );
        assert_closed_audits(&handle, sender, recipient, attempt + 1).await?;
    }
    assert!(
        handle
            .launch()
            .fetch_thread_messages(thread.id)
            .await?
            .is_empty()
    );
    assert!(
        handle
            .launch()
            .insert_threaded_message_with_context(
                crate::store::persistence::mailbox::ThreadedMessageInsert {
                    message_uuid: Uuid::new_v4(),
                    to_session_id: None,
                    from_session_id: None,
                    body: "post only",
                    thread_id: Some(thread.id),
                    in_reply_to_id: None,
                },
                &items,
            )
            .await?
            .is_some()
    );
    assert_closed_audits(&handle, sender, recipient, 2).await?;
    launch(&handle, &run, recipient, "worker-reopened").await?;
    assert!(
        handle
            .launch()
            .insert_message_with_context(
                Uuid::new_v4(),
                recipient,
                Some(sender),
                "open again",
                &items
            )
            .await?
            .is_some()
    );
    assert_closed_audits(&handle, sender, recipient, 2).await?;
    db.cleanup().await
}
