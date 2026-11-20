use super::*;

const CONTINUE: &str = r#"{"model":"m","messages":[{"role":"user","content":"msg_a is named here too"}],"thread":{"type":"continue","previous_message_id":"msg_a"}}"#;

#[test]
fn rewrites_only_the_thread_previous_message_id() -> anyhow::Result<()> {
    let out = with_previous_message_id(CONTINUE.as_bytes(), "msg_a", "msg_b")
        .ok_or_else(|| anyhow::anyhow!("not rewritten"))?;
    assert_eq!(
        std::str::from_utf8(&out)?,
        CONTINUE.replace(
            r#""previous_message_id":"msg_a""#,
            r#""previous_message_id":"msg_b""#
        )
    );
    assert_eq!(continued_from(&out).as_deref(), Some("msg_b"));
    Ok(())
}

#[test]
fn keeps_the_client_bytes_around_the_thread_object() -> anyhow::Result<()> {
    let spaced =
        r#"{"thread": {"previous_message_id": "msg_a", "type": "continue"}, "messages": []}"#;
    let out = with_previous_message_id(spaced.as_bytes(), "msg_a", "msg_b")
        .ok_or_else(|| anyhow::anyhow!("not rewritten"))?;
    assert_eq!(
        std::str::from_utf8(&out)?,
        r#"{"thread": {"previous_message_id": "msg_b", "type": "continue"}, "messages": []}"#
    );
    Ok(())
}

#[test]
fn a_body_naming_another_message_is_left_alone() {
    assert!(with_previous_message_id(CONTINUE.as_bytes(), "msg_z", "msg_b").is_none());
}

#[test]
fn a_create_or_threadless_body_continues_from_nothing() {
    assert_eq!(
        continued_from(br#"{"messages":[],"thread":{"type":"create"}}"#),
        None
    );
    assert_eq!(continued_from(br#"{"messages":[]}"#), None);
}
