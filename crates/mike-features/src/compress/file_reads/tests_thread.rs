use serde_json::{Value, json};

use crate::compress::history::earlier_from_messages;
use crate::wire::manifest::StageOutcome;

use super::reduce_file_reads;
use super::tests_support::{TestPolicy, big_file_content};

fn call(id: &str) -> Value {
    json!({"role": "assistant", "content": [
        {"type": "tool_use", "id": id, "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
    ]})
}

fn result(id: &str, content: &str) -> Value {
    json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": id, "content": content}
    ]})
}

fn prompt(text: &str) -> Value {
    json!({"role": "user", "content": [{"type": "text", "text": text}]})
}

#[test]
fn file_read_result_continue_body_with_earlier_tool_use_reduces_like_full_body()
-> anyhow::Result<()> {
    let content = big_file_content(200);
    let go = prompt("go");
    let full = serde_json::to_vec(&json!({"messages": [
        go.clone(), call("t1"), result("t1", &content), prompt("continue")
    ]}))?;
    let thread = serde_json::to_vec(&json!({
        "thread": {"type": "continue", "previous_message_id": "msg_1"},
        "messages": [result("t1", &content), prompt("continue")]
    }))?;
    let earlier = earlier_from_messages(&[go, call("t1")])?;
    let tp = TestPolicy::default();

    let whole = reduce_file_reads(&full, &tp.policy(), &crate::conversation::WHOLE_BODY);
    let chained = reduce_file_reads(&thread, &tp.policy(), &earlier);
    let bare = reduce_file_reads(&thread, &tp.policy(), &crate::conversation::WHOLE_BODY);

    assert_eq!(whole.stage.outcome, StageOutcome::Applied);
    assert_eq!(chained.stage.outcome, StageOutcome::Applied);
    assert_ne!(bare.stage.outcome, StageOutcome::Applied);
    assert_eq!(
        chained.stage.targets[0].reason_code,
        whole.stage.targets[0].reason_code
    );
    assert_eq!(
        chained.stage.targets[0].json_path.as_deref(),
        Some("/messages/0/content/0/content")
    );
    let whole_sent: Value = serde_json::from_slice(&whole.candidate)?;
    let chained_sent: Value = serde_json::from_slice(&chained.candidate)?;
    assert_eq!(chained_sent["messages"][0], whole_sent["messages"][2]);
    assert_eq!(
        chained_sent["thread"],
        json!({"type": "continue", "previous_message_id": "msg_1"})
    );
    Ok(())
}

#[test]
fn file_read_result_continue_body_with_earlier_whole_reread_is_superseded() -> anyhow::Result<()> {
    let content = big_file_content(200);
    let go = prompt("go");
    let later_call = call("t2");
    let later_result = result("t2", &big_file_content(150));
    let thread = serde_json::to_vec(&json!({
        "thread": {"type": "continue", "previous_message_id": "msg_1"},
        "messages": [result("t1", &content), later_call, later_result, prompt("continue")]
    }))?;
    let earlier = earlier_from_messages(&[go, call("t1")])?;
    let tp = TestPolicy::default();
    let chained = reduce_file_reads(&thread, &tp.policy(), &earlier);
    let first = chained
        .stage
        .targets
        .iter()
        .find(|t| t.tool_use_id.as_deref() == Some("t1"))
        .ok_or_else(|| anyhow::anyhow!("t1 target missing"))?;
    assert_eq!(first.outcome, StageOutcome::Applied);
    assert_eq!(first.reason_code, super::reason::FILE_READ_SUPERSEDED);
    Ok(())
}
