use chrono::{TimeZone, Utc};

use crate::store::persistence::mailbox::MailboxEnvelope;
use crate::wire::mail_text::is_mail_text;

const FROM: &str = "11111111-2222-3333-4444-555555555555";

fn envelope<'a>(
    from: Option<&'a str>,
    body: &'a str,
    queued: &'a chrono::DateTime<Utc>,
) -> MailboxEnvelope<'a> {
    MailboxEnvelope {
        from_session_uuid: from,
        queued_at: queued,
        body,
        to_session_uuid: uuid::Uuid::nil(),
        message_uuid: uuid::Uuid::from_u128(0xbbbb_bbbb_5555_6666_7777_8888_8888_8888),
        thread: None,
    }
}

fn queued() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 6, 14, 3, 9).unwrap()
}

#[test]
fn a_mail_is_its_sender_and_its_body() {
    let at = queued();
    assert_eq!(
        envelope(Some(FROM), "okay", &at).render(),
        format!("<system-reminder>\nMail from session {FROM}: okay\n</system-reminder>")
    );
    assert_eq!(
        envelope(Some(FROM), "okay", &at).letter_as(Some("judge-backlog (session x)")),
        "Mail from judge-backlog (session x): okay"
    );
}

#[test]
fn a_mail_carries_no_message_id_time_reply_line_or_trust_framing() {
    let at = queued();
    let out = envelope(Some(FROM), "x", &at).render();
    for banned in [
        "bbbbbbbb",
        "14:03",
        "Reply:",
        "request_reply",
        "not an instruction",
        "ack_session_message",
    ] {
        assert!(!out.contains(banned), "{banned}: {out}");
    }
}

#[test]
fn a_mail_without_a_sender_says_so() {
    let at = queued();
    assert_eq!(
        envelope(None, "heads up", &at).letter_as(None),
        "Mail from an unknown sender: heads up"
    );
}

#[test]
fn body_is_inside_verbatim_newlines_and_all() {
    let at = queued();
    let body = "please rerun the flaky suite\nwith --retries 2";
    assert!(envelope(None, body, &at).render().contains(body));
}

#[test]
fn every_shape_of_mail_is_recognised_as_mail_and_typed_text_is_not() {
    let at = queued();
    for mail in [
        envelope(Some(FROM), "x", &at).render(),
        envelope(None, "x", &at).render(),
        envelope(None, "x", &at).letter_as(Some("mike")),
        envelope(Some(FROM), "x", &at).letter_as(Some("worker-T1 (task T1, session y)")),
    ] {
        assert!(is_mail_text(&mail), "{mail}");
    }
    assert!(!is_mail_text("Mail from Bob: did you see the invoice?"));
}
