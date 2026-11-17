use serde_json::{Value, json};

use crate::compress::recovery::CapturedResults;
use crate::wire::manifest::StageOutcome;

use super::{REDUCTION_MARKER_TAG, ShellOutputLimits, ShellOutputPolicy, reduce_shell_output};

fn long_output(lines: usize) -> String {
    let mut out: Vec<String> = (1..=lines)
        .map(|i| {
            format!("crates/mike/src/module_{i}/file_{i}.rs: a listing line long enough to count")
        })
        .collect();
    out[lines / 2] = "error: build failed in the middle".to_string();
    out.join("\n")
}

fn body_with(name: &str, content: &Value, answers: usize) -> Value {
    let mut messages = vec![
        json!({"role": "user", "content": [{"type": "text", "text": "go"}]}),
        json!({"role": "assistant", "content": [
            {"type": "tool_use", "id": "t1", "name": name, "input": {"command": "cargo build"}}
        ]}),
        json!({"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": content}
        ]}),
    ];
    for _ in 0..answers {
        messages.push(json!({"role": "assistant", "content": [{"type": "text", "text": "noted"}]}));
        messages.push(json!({"role": "user", "content": [{"type": "text", "text": "continue"}]}));
    }
    json!({"model": "m", "system": [{"type": "text", "text": "SYSTEM"}], "messages": messages})
}

fn limits() -> ShellOutputLimits {
    ShellOutputLimits::default()
}

#[test]
fn reduce_shell_output_shortens_an_older_bash_result_and_recovers_it_exactly() -> anyhow::Result<()>
{
    let original = long_output(200);
    let body = body_with("Bash", &Value::String(original.clone()), 1);
    let raw = serde_json::to_vec(&body)?;
    let policy = ShellOutputPolicy::new(limits()).with_capture(&raw);
    let reduction = reduce_shell_output(&raw, &policy, &crate::conversation::WHOLE_BODY);
    assert_eq!(reduction.stage.outcome, StageOutcome::Applied);
    let sent: Value = serde_json::from_slice(&reduction.candidate)?;
    let text = sent
        .pointer("/messages/2/content/0/content")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(text.contains(REDUCTION_MARKER_TAG));
    assert!(text.contains("error: build failed in the middle"));
    assert!(text.len() * 2 < original.len());
    let target = &reduction.stage.targets[0];
    let reference: Value = serde_json::from_str(target.recovery_ref.as_deref().unwrap_or("null"))?;
    assert!(reference.get("id").is_none());
    let later_raw = serde_json::to_vec(&body_with("Bash", &Value::String(original.clone()), 3))?;
    let later = reduce_shell_output(
        &later_raw,
        &ShellOutputPolicy::new(limits()).with_capture(&later_raw),
        &crate::conversation::WHOLE_BODY,
    );
    let later: Value = serde_json::from_slice(&later.candidate)?;
    assert_eq!(later["messages"][2], sent["messages"][2]);
    assert!(text.contains(&target.recovery_ref.clone().unwrap_or_default()));
    let capture = CapturedResults::new(&raw);
    let pointer = capture
        .pointer("t1", reference["sha256"].as_str().unwrap_or(""))
        .ok_or_else(|| anyhow::anyhow!("the reference must resolve in the captured body"))?;
    assert_eq!(
        body.pointer(pointer).and_then(Value::as_str),
        Some(original.as_str())
    );
    let mut restored = sent.clone();
    restored["messages"][2]["content"][0]["content"] = Value::String(original);
    assert_eq!(restored, body, "nothing outside the Bash result changes");
    Ok(())
}

#[test]
fn reduce_shell_output_newest_result_is_cut_on_arrival_identically_to_later_requests()
-> anyhow::Result<()> {
    let content = Value::String(long_output(200));
    let arrival_raw = serde_json::to_vec(&body_with("Bash", &content, 0))?;
    let arrival = reduce_shell_output(
        &arrival_raw,
        &ShellOutputPolicy::new(limits()).with_capture(&arrival_raw),
        &crate::conversation::WHOLE_BODY,
    );
    assert_eq!(arrival.stage.outcome, StageOutcome::Applied);
    let later_raw = serde_json::to_vec(&body_with("Bash", &content, 4))?;
    let later = reduce_shell_output(
        &later_raw,
        &ShellOutputPolicy::new(limits()).with_capture(&later_raw),
        &crate::conversation::WHOLE_BODY,
    );
    let arrival: Value = serde_json::from_slice(&arrival.candidate)?;
    let later: Value = serde_json::from_slice(&later.candidate)?;
    assert_eq!(arrival["messages"][2], later["messages"][2]);
    Ok(())
}

#[test]
fn reduce_shell_output_file_reads_and_listings_stay_whole() -> anyhow::Result<()> {
    for (command, reason) in [
        ("sed -n '1,400p' src/lib.rs", "command_reads_file"),
        ("rg -n needle crates", "command_lists"),
    ] {
        let mut body = body_with("Bash", &Value::String(long_output(200)), 0);
        body["messages"][1]["content"][0]["input"]["command"] = json!(command);
        let raw = serde_json::to_vec(&body)?;
        let reduction = reduce_shell_output(
            &raw,
            &ShellOutputPolicy::new(limits()).with_capture(&raw),
            &crate::conversation::WHOLE_BODY,
        );
        assert_eq!(reduction.candidate.as_ref(), raw.as_slice(), "{command}");
        assert_eq!(reduction.stage.targets[0].reason_code, reason);
    }
    Ok(())
}

#[test]
fn reduce_shell_output_other_tools_stay_whole() -> anyhow::Result<()> {
    let raw = serde_json::to_vec(&body_with("Read", &Value::String(long_output(200)), 0))?;
    let reduction = reduce_shell_output(
        &raw,
        &ShellOutputPolicy::new(limits()).with_capture(&raw),
        &crate::conversation::WHOLE_BODY,
    );
    assert_eq!(reduction.candidate.as_ref(), raw.as_slice());
    assert!(reduction.stage.targets.is_empty());
    Ok(())
}

#[test]
fn reduce_shell_output_identical_outputs_are_each_cut_the_same_way() -> anyhow::Result<()> {
    let output = long_output(200);
    let mut body = body_with("Bash", &Value::String(output.clone()), 0);
    let messages = body["messages"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("messages"))?;
    messages.push(json!({"role": "assistant", "content": [
        {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "cargo build"}}
    ]}));
    messages.push(json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "t2", "content": output}
    ]}));
    let raw = serde_json::to_vec(&body)?;
    let reduction = reduce_shell_output(
        &raw,
        &ShellOutputPolicy::new(limits()).with_capture(&raw),
        &crate::conversation::WHOLE_BODY,
    );
    let applied = reduction
        .stage
        .targets
        .iter()
        .filter(|t| t.outcome == StageOutcome::Applied)
        .count();
    assert_eq!(applied, 2);
    let sent: Value = serde_json::from_slice(&reduction.candidate)?;
    let first = sent
        .pointer("/messages/2/content/0/content")
        .and_then(Value::as_str);
    let second = sent
        .pointer("/messages/4/content/0/content")
        .and_then(Value::as_str);
    assert!(first.is_some_and(|t| t.contains(REDUCTION_MARKER_TAG)));
    assert!(second.is_some_and(|t| t.contains(REDUCTION_MARKER_TAG)));
    Ok(())
}

#[test]
fn reduce_shell_output_skips_injected_error_and_small_results() -> anyhow::Result<()> {
    let injected = format!(
        "{}\n<system-reminder>hook text</system-reminder>",
        long_output(200)
    );
    let error_body = {
        let mut b = body_with("Bash", &Value::String(long_output(200)), 1);
        b["messages"][2]["content"][0]["is_error"] = Value::Bool(true);
        b
    };
    let cases = [
        (
            body_with("Bash", &Value::String(injected), 1),
            limits(),
            "injected_content",
        ),
        (error_body, limits(), "error_result"),
        (
            body_with("Bash", &Value::String("short".into()), 1),
            limits(),
            "small_result",
        ),
    ];
    for (body, limits, reason) in cases {
        let raw = serde_json::to_vec(&body)?;
        let reduction = reduce_shell_output(
            &raw,
            &ShellOutputPolicy::new(limits).with_capture(&raw),
            &crate::conversation::WHOLE_BODY,
        );
        assert_eq!(reduction.candidate.as_ref(), raw.as_slice(), "{reason}");
        assert_eq!(reduction.stage.targets[0].reason_code, reason);
    }
    Ok(())
}

#[test]
fn reduce_shell_output_without_capture_is_not_recoverable_and_skips() -> anyhow::Result<()> {
    let raw = serde_json::to_vec(&body_with("Bash", &Value::String(long_output(200)), 1))?;
    let reduction = reduce_shell_output(
        &raw,
        &ShellOutputPolicy::new(limits()),
        &crate::conversation::WHOLE_BODY,
    );
    assert_eq!(reduction.candidate.as_ref(), raw.as_slice());
    assert_eq!(
        reduction.stage.targets[0].reason_code,
        "original_not_recoverable"
    );
    let malformed = reduce_shell_output(
        b"not json",
        &ShellOutputPolicy::new(limits()),
        &crate::conversation::WHOLE_BODY,
    );
    assert_eq!(malformed.stage.outcome, StageOutcome::Skipped);
    Ok(())
}
