use super::{ThreadKind, ThreadLink};

fn link(body: &[u8]) -> anyhow::Result<ThreadLink> {
    ThreadLink::from_body(body).ok_or_else(|| anyhow::anyhow!("no thread link"))
}

#[test]
fn from_body_continue_with_previous_id_reads_both() -> anyhow::Result<()> {
    let link = link(
        br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"thread":{"type":"continue","previous_message_id":"msg_01"}}"#,
    )?;
    assert_eq!(link.kind, ThreadKind::Continue);
    assert_eq!(link.previous_message_id.as_deref(), Some("msg_01"));
    assert!(link.continues());
    Ok(())
}

#[test]
fn from_body_create_reads_kind_without_previous_id() -> anyhow::Result<()> {
    let link = link(br#"{"thread":{"type":"create"},"messages":[]}"#)?;
    assert_eq!(link.kind, ThreadKind::Create);
    assert_eq!(link.previous_message_id, None);
    Ok(())
}

#[test]
fn from_body_without_thread_is_none() {
    assert_eq!(ThreadLink::from_body(br#"{"messages":[]}"#), None);
}

#[test]
fn from_body_unknown_kind_is_other_and_keeps_previous_id() -> anyhow::Result<()> {
    let link = link(br#"{"thread":{"type":"fork","previous_message_id":"msg_1"}}"#)?;
    assert_eq!(link.kind, ThreadKind::Other);
    assert!(!link.is_known());
    assert!(!link.continues());
    assert_eq!(link.previous_message_id.as_deref(), Some("msg_1"));
    Ok(())
}

#[test]
fn from_body_thread_without_type_is_other() -> anyhow::Result<()> {
    assert_eq!(link(br#"{"thread":{}}"#)?.kind, ThreadKind::Other);
    Ok(())
}

#[test]
fn from_body_not_json_is_none() {
    assert_eq!(ThreadLink::from_body(b"not json"), None);
}

#[test]
fn from_body_escaped_previous_id_is_decoded() -> anyhow::Result<()> {
    let body = [
        &br#"{"thread":{"type":"continue","previous_message_id":"msg_"#[..],
        &[b'\\', b'u'][..],
        &br#"0031"}}"#[..],
    ]
    .concat();
    assert_eq!(link(&body)?.previous_message_id.as_deref(), Some("msg_1"));
    Ok(())
}
