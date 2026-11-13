use super::tests_support::*;
use super::*;
use serde_json::json;

#[test]
fn reduction_carries_verified_capture_references() -> anyhow::Result<()> {
    let text = big_file_content(180);
    let raw = raw_in_history(json!({"messages":[
        {"role":"assistant","content":[{"type":"tool_use","id":"one","name":"Read","input":{"file_path":"/repo/one"}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"one","content":text}]}
    ]}))?;
    let tp = TestPolicy::default();
    let policy = tp.policy().with_capture(&raw);
    let result = reduce_file_reads(&raw, &policy);
    let target = result
        .stage
        .targets
        .iter()
        .find(|t| t.outcome == StageOutcome::Applied)
        .ok_or_else(|| anyhow::anyhow!("no reduction"))?;
    let reference: Value = serde_json::from_str(
        target
            .recovery_ref
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing reference"))?,
    )?;
    assert!(reference.get("id").is_none());
    assert_eq!(reference["tool_use_id"], "one");
    assert_eq!(reference["sha256"], sha256_hex(text.as_bytes()));
    assert!(std::str::from_utf8(&result.candidate)?.contains("get_request_body"));
    let changed = String::from_utf8(raw.clone())?.replace("line 90 of", "edited 90 of");
    let declined = reduce_file_reads(changed.as_bytes(), &policy);
    assert_eq!(declined.candidate.as_ref(), changed.as_bytes());
    assert!(
        declined
            .stage
            .targets
            .iter()
            .any(|t| t.reason_code == "original_not_recoverable")
    );
    Ok(())
}

#[test]
fn reapplied_cut_is_byte_identical_on_a_later_request() -> anyhow::Result<()> {
    let history = json!({"messages":[
        {"role":"assistant","content":[{"type":"tool_use","id":"one","name":"Read","input":{"file_path":"/repo/one"}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"one","content":big_file_content(180)}]}
    ]});
    let earlier = raw_in_history(history.clone())?;
    let mut longer: Value = serde_json::from_slice(&raw_in_history(history)?)?;
    if let Some(messages) = longer["messages"].as_array_mut() {
        messages.push(json!({"role":"assistant","content":[{"type":"text","text":"later"}]}));
        messages.push(json!({"role":"user","content":[{"type":"text","text":"again"}]}));
    }
    let later = serde_json::to_vec(&longer)?;
    let tp = TestPolicy::default();
    let first = reduce_file_reads(&earlier, &tp.policy().with_capture(&earlier));
    let second = reduce_file_reads(&later, &tp.policy().with_capture(&later));
    assert_eq!(first.stage.outcome, StageOutcome::Applied);
    assert_eq!(second.stage.outcome, StageOutcome::Applied);
    let first: Value = serde_json::from_slice(&first.candidate)?;
    let second: Value = serde_json::from_slice(&second.candidate)?;
    assert_eq!(first["messages"][1], second["messages"][1]);
    Ok(())
}
