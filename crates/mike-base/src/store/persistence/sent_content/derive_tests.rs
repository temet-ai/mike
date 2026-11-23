use super::*;
use crate::store::persistence::messages::canonical_clean_messages;
use crate::store::persistence::reconstruct::strip_bulk_arrays;

fn envelope_text(original: &[u8], sent: &[u8]) -> Option<String> {
    let original_envelope = strip_bulk_arrays(original);
    sent_envelope(original, original_envelope.as_deref(), sent)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn keys(messages: &[CanonicalMessage]) -> Vec<MessageKey<'_>> {
    messages.iter().map(MessageKey::from).collect()
}

#[test]
fn sent_envelope_identical_bodies_is_none() {
    let body =
        br#"{"system":"S","messages":[{"role":"user","content":"a"}],"tools":[{"name":"Read"}]}"#;
    assert_eq!(envelope_text(body, body), None);
}

#[test]
fn sent_envelope_message_only_edit_is_none() {
    let original =
        br#"{"system":"S","messages":[{"role":"user","content":"a"}],"tools":[{"name":"Read"}]}"#;
    let sent = br#"{"system":"S","messages":[{"role":"user","content":"a EDITED"}],"tools":[{"name":"Read"}]}"#;
    assert_eq!(envelope_text(original, sent), None);
}

#[test]
fn sent_envelope_system_edit_strips_messages_and_unchanged_tools() {
    let original =
        br#"{"system":"S","messages":[{"role":"user","content":"a"}],"tools":[{"name":"Read"}]}"#;
    let sent =
        br#"{"system":"S2","messages":[{"role":"user","content":"a"}],"tools":[{"name":"Read"}]}"#;
    assert_eq!(
        envelope_text(original, sent).as_deref(),
        Some(r#"{"system":"S2","messages":[],"tools":[]}"#)
    );
}

#[test]
fn sent_envelope_changed_tools_stay_inline() {
    let original = br#"{"messages":[{"role":"user","content":"a"}],"tools":[{"name":"Read"}]}"#;
    let sent = br#"{"messages":[{"role":"user","content":"a"}],"tools":[{"name":"Read"},{"name":"mike"}]}"#;
    assert_eq!(
        envelope_text(original, sent).as_deref(),
        Some(r#"{"messages":[],"tools":[{"name":"Read"},{"name":"mike"}]}"#)
    );
}

#[test]
fn sent_envelope_emptied_tools_keep_a_space_so_they_are_not_refilled() {
    let original = br#"{"messages":[],"tools":[{"name":"Read"}]}"#;
    let sent = br#"{"messages":[],"tools":[]}"#;
    assert_eq!(
        envelope_text(original, sent).as_deref(),
        Some(r#"{"messages":[],"tools":[ ]}"#)
    );
}

#[test]
fn whole_sent_envelope_keeps_messages_inline() {
    let original = br#"{"messages":[{"role":"user","content":"a"}],"tools":[{"name":"Read"}]}"#;
    let sent = br#"{"messages":[{"role":"robot","content":"a"}],"tools":[]}"#;
    assert_eq!(
        whole_sent_envelope(original, sent),
        br#"{"messages":[{"role":"robot","content":"a"}],"tools":[ ]}"#.to_vec()
    );
}

#[test]
fn plan_sent_messages_same_count_keeps_only_changed_ords() -> anyhow::Result<()> {
    let original = canonical_clean_messages(
        br#"{"messages":[{"role":"user","content":"a"},{"role":"assistant","content":"b"},{"role":"user","content":"c"}]}"#,
    )?;
    let sent = canonical_clean_messages(
        br#"{"messages":[{"role":"user","content":"a"},{"role":"assistant","content":"B"},{"role":"user","content":"c"}]}"#,
    )?;
    let plan = plan_sent_messages(&keys(&original), sent);
    assert_eq!(plan.count, None);
    assert_eq!(plan.rows.iter().map(|m| m.ord).collect::<Vec<_>>(), vec![1]);
    Ok(())
}

#[test]
fn plan_sent_messages_moved_cache_control_is_a_row() -> anyhow::Result<()> {
    let original = canonical_clean_messages(
        br#"{"messages":[{"role":"user","content":[{"type":"text","text":"a"}]}]}"#,
    )?;
    let sent = canonical_clean_messages(
        br#"{"messages":[{"role":"user","content":[{"type":"text","text":"a","cache_control":{"type":"ephemeral"}}]}]}"#,
    )?;
    let plan = plan_sent_messages(&keys(&original), sent);
    assert_eq!(plan.rows.len(), 1);
    assert!(plan.rows[0].cache_control.is_some());
    Ok(())
}

#[test]
fn plan_sent_messages_count_change_keeps_every_message() -> anyhow::Result<()> {
    let original = canonical_clean_messages(br#"{"messages":[{"role":"user","content":"a"}]}"#)?;
    let sent = canonical_clean_messages(
        br#"{"messages":[{"role":"user","content":"a"},{"role":"user","content":"mail"}]}"#,
    )?;
    let plan = plan_sent_messages(&keys(&original), sent);
    assert_eq!(plan.count, Some(2));
    assert_eq!(
        plan.rows.iter().map(|m| m.ord).collect::<Vec<_>>(),
        vec![0, 1]
    );
    Ok(())
}

#[test]
fn plan_sent_messages_identical_messages_is_empty() -> anyhow::Result<()> {
    let body = br#"{"messages":[{"role":"user","content":"a"}]}"#;
    let plan = plan_sent_messages(
        &keys(&canonical_clean_messages(body)?),
        canonical_clean_messages(body)?,
    );
    assert_eq!(plan.count, None);
    assert!(plan.rows.is_empty());
    Ok(())
}
