use serde_json::{Value, json};

use crate::compress::history::earlier_from_messages;
use crate::conversation::WHOLE_BODY;
use crate::wire::manifest::StageOutcome;

use super::{ShellOutputLimits, ShellOutputPolicy, reduce_shell_output};

fn long_output(lines: usize) -> String {
    let mut out: Vec<String> = (1..=lines)
        .map(|i| {
            format!("crates/mike/src/module_{i}/file_{i}.rs: a listing line long enough to count")
        })
        .collect();
    out[lines / 2] = "error: build failed in the middle".to_string();
    out.join("\n")
}

fn prompt(text: &str) -> Value {
    json!({"role": "user", "content": [{"type": "text", "text": text}]})
}

fn bash_call() -> Value {
    json!({"role": "assistant", "content": [
        {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cargo build"}}
    ]})
}

fn bash_result(content: &str) -> Value {
    json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "t1", "content": content}
    ]})
}

#[test]
fn shell_output_continue_body_with_earlier_bash_tool_use_reduces_like_full_body()
-> anyhow::Result<()> {
    let content = long_output(200);
    let go = prompt("go");
    let full = serde_json::to_vec(&json!({"messages": [
        go.clone(), bash_call(), bash_result(&content), prompt("continue")
    ]}))?;
    let thread = serde_json::to_vec(&json!({
        "thread": {"type": "continue", "previous_message_id": "msg_1"},
        "messages": [bash_result(&content), prompt("continue")]
    }))?;
    let earlier = earlier_from_messages(&[go, bash_call()])?;
    let limits = ShellOutputLimits::default;

    let whole = reduce_shell_output(
        &full,
        &ShellOutputPolicy::new(limits()).with_capture(&full),
        &WHOLE_BODY,
    );
    let chained = reduce_shell_output(
        &thread,
        &ShellOutputPolicy::new(limits()).with_capture(&thread),
        &earlier,
    );
    let bare = reduce_shell_output(
        &thread,
        &ShellOutputPolicy::new(limits()).with_capture(&thread),
        &WHOLE_BODY,
    );

    assert_eq!(whole.stage.outcome, StageOutcome::Applied);
    assert_eq!(chained.stage.outcome, StageOutcome::Applied);
    assert_eq!(bare.stage.outcome, StageOutcome::Unchanged);
    let whole_sent: Value = serde_json::from_slice(&whole.candidate)?;
    let chained_sent: Value = serde_json::from_slice(&chained.candidate)?;
    assert_eq!(chained_sent["messages"][0], whole_sent["messages"][2]);
    assert_eq!(
        chained_sent["thread"],
        json!({"type": "continue", "previous_message_id": "msg_1"})
    );
    Ok(())
}
