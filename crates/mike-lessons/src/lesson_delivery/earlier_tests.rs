use std::sync::Arc;

use super::held::{HeldTag, held_lesson_tags};
use crate::conversation::{EarlierTurns, body_messages};

const EARLIER: &str = r#"[{"role":"user","content":"start"},{"role":"assistant","content":[{"type":"text","text":"<system-reminder>\n[lesson:12 rev:3] [correction] a\n[lesson:7] [convention] b\n</system-reminder>"}]}]"#;
const NEW: &str =
    r#"[{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}]"#;

fn earlier_turns() -> anyhow::Result<EarlierTurns> {
    let body = format!(r#"{{"messages":{EARLIER}}}"#);
    let messages = body_messages(body.as_bytes()).unwrap_or_default();
    Ok(EarlierTurns::from_chain(Arc::from(messages)))
}

#[test]
fn held_lesson_tags_continue_body_with_earlier_tags_match_full_body() -> anyhow::Result<()> {
    let continue_body = format!(
        r#"{{"thread":{{"type":"continue","previous_message_id":"msg_1"}},"messages":{NEW}}}"#
    );
    let earlier = earlier_turns()?;
    let held = held_lesson_tags(continue_body.as_bytes(), &earlier);
    let full = r#"{"messages":[{"role":"user","content":"start"},{"role":"assistant","content":[{"type":"text","text":"<system-reminder>\n[lesson:12 rev:3] [correction] a\n[lesson:7] [convention] b\n</system-reminder>"}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}]}"#;
    let expected = held_lesson_tags(full.as_bytes(), &crate::conversation::WHOLE_BODY);
    assert_eq!(held.len(), 2);
    assert!(held.contains(&HeldTag {
        id: 12,
        revision: Some(3)
    }));
    assert!(held.contains(&HeldTag {
        id: 7,
        revision: None
    }));
    assert_eq!(held.len(), expected.len());
    assert!(
        held_lesson_tags(continue_body.as_bytes(), &crate::conversation::WHOLE_BODY).is_empty()
    );
    Ok(())
}
