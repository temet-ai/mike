use std::sync::Arc;

use super::*;
use crate::conversation::body_messages;

#[test]
fn carries_marker_continue_body_with_card_in_earlier_turns_is_true() -> anyhow::Result<()> {
    let earlier_body = format!(
        r#"{{"messages":[{{"role":"user","content":"hello {KEEPER_MARKER_OPEN}0123] card"}},{{"role":"assistant","content":"ok"}}]}}"#
    );
    let earlier = EarlierTurns::from_chain(Arc::from(
        body_messages(earlier_body.as_bytes()).unwrap_or_default(),
    ));
    let continue_body = br#"{"thread":{"type":"continue","previous_message_id":"msg_1"},"messages":[{"role":"user","content":"next"}]}"#;
    assert!(carries_marker_with(continue_body, &earlier));
    assert!(!carries_marker_with(
        continue_body,
        &EarlierTurns::whole_body()
    ));
    let full = format!(
        r#"{{"messages":[{{"role":"user","content":"hello {KEEPER_MARKER_OPEN}0123] card"}},{{"role":"assistant","content":"ok"}},{{"role":"user","content":"next"}}]}}"#
    );
    assert!(carries_marker(full.as_bytes()));
    Ok(())
}
