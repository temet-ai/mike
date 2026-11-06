use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::{CHECKOUT, StatusCode, fleet, test_seed};
use crate::change_record::ChangeKind;
use crate::store::persistence::mailbox::MailboxEnvelope;

#[tokio::test]
async fn the_envelope_names_a_fleet_seat_and_its_task() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let label = f
        .env
        .persist
        .sender_label(f.t1.session)
        .await
        .ok_or_else(|| anyhow::anyhow!("a fleet seat has a label"))?;
    assert_eq!(
        label,
        format!("worker-T1 (task T1, session {})", f.t1.session)
    );
    let envelope = render(f.t1.session, f.t2.session, Some(&label));
    eprintln!("rendered envelope for a fleet seat:\n{envelope}");
    assert_eq!(
        envelope,
        format!(
            "<system-reminder>\nMail from worker-T1 (task T1, session {from}): please look at the mail path\n</system-reminder>",
            from = f.t1.session
        )
    );
    f.env.cleanup().await
}

#[tokio::test]
async fn the_envelope_names_a_named_session_outside_a_fleet() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    let id =
        test_seed::session(f.env.persist.pool(), f.env.project, Some("/tmp/elsewhere")).await?;
    let named = f
        .env
        .persist
        .session_identities(&[id])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no identity"))?
        .uuid;
    f.env
        .persist
        .set_session_goal(
            crate::store::persistence::session_goals::SetSessionIdentity {
                session_uuid: named,
                display_name: Some("reviewer"),
                role: None,
                goal: None,
                next_steps: None,
                instructions: None,
                set_by: "test",
            },
        )
        .await?;
    let label = f.env.persist.sender_label(named).await;
    assert_eq!(label, Some(format!("reviewer (session {named})")));
    f.env.cleanup().await
}

fn render(from: Uuid, to: Uuid, label: Option<&str>) -> String {
    let queued = Utc
        .with_ymd_and_hms(2026, 10, 6, 14, 3, 9)
        .single()
        .unwrap_or_default();
    let from = from.to_string();
    MailboxEnvelope {
        from_session_uuid: Some(&from),
        queued_at: &queued,
        body: "please look at the mail path",
        to_session_uuid: to,
        message_uuid: Uuid::from_u128(0xb),
        thread: None,
    }
    .render_as(label)
}

#[tokio::test]
async fn the_answer_carries_the_tail_only_when_the_toggle_is_on() -> anyhow::Result<()> {
    let Some(f) = fleet().await? else {
        return Ok(());
    };
    f.env.tail(false);
    let (_, off) = f.env.send(Some(f.t1.session), "task:T2", None).await;
    assert!(off.get("since_you_last_looked").is_none(), "{off}");

    let other_id = test_seed::session(f.env.persist.pool(), f.env.project, None).await?;
    let mut baseline = test_seed::event(f.env.project, ChangeKind::Edit, &["crates/a/src/old.rs"]);
    baseline.checkout = CHECKOUT.to_owned();
    baseline.session_id = Some(other_id);
    f.env.persist.insert_change_events(&[baseline]).await?;

    f.env.tail(true);
    let (_, primed) = f.env.send(Some(f.t1.session), "task:T2", None).await;
    assert!(primed.get("since_you_last_looked").is_none(), "{primed}");

    let mut event = test_seed::event(f.env.project, ChangeKind::Edit, &["crates/a/src/new.rs"]);
    event.checkout = CHECKOUT.to_owned();
    event.session_id = Some(other_id);
    f.env.persist.insert_change_events(&[event]).await?;

    let (status, on) = f.env.send(Some(f.t1.session), "task:T2", None).await;
    assert_eq!(status, StatusCode::CREATED, "{on}");
    let lines = on["since_you_last_looked"].as_array();
    assert!(
        lines.is_some_and(|l| l
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s.contains("new.rs")))),
        "{on}"
    );
    f.env.cleanup().await
}
