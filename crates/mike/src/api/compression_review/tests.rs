use super::{Flag, store, validate_flag};
use serde_json::json;

#[test]
fn target_identity_matches_python_queue() {
    assert_eq!(
        store::target_id("tool_result", 473363, Some(808536)),
        "d03f5368cef5e445015a2b7b04b3edaef1f72dbf60400e24ad53c1ae8969ef48"
    );
}

#[test]
fn persisted_events_keep_nomination_and_review_history() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("review");
    let mut file = store::locked(&path)?;
    store::append(
        &mut file,
        json!({"action":"flag", "id":"a", "target":{"kind":"response", "request_id":42, "tool_use_id":null}, "reason":"Repeated text", "evidence":"capture", "actor":"test"}),
    )?;
    store::append(
        &mut file,
        json!({"action":"review", "id":"a", "status":"dismissed", "note":"Needed", "actor":"test"}),
    )?;
    store::append(
        &mut file,
        json!({"action":"flag", "id":"a", "target":{"kind":"response", "request_id":42, "tool_use_id":null}, "reason":"New evidence", "actor":"test"}),
    )?;
    drop(file);
    let file = store::locked(&path)?;
    let items = store::items(&store::events(&file)?)?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["status"], "dismissed");
    assert_eq!(items[0]["history"].as_array().map(Vec::len), Some(3));
    Ok(())
}

#[test]
fn invalid_tool_identity_and_blank_reason_are_rejected() {
    let mut flag = Flag {
        kind: "tool_result".into(),
        request_id: 1,
        tool_use_id: None,
        reason: "reason".into(),
        evidence: "evidence".into(),
        actor: "test".into(),
    };
    assert!(validate_flag(&flag).is_err());
    flag.tool_use_id = Some(2);
    assert!(validate_flag(&flag).is_ok());
    flag.reason = " ".into();
    assert!(validate_flag(&flag).is_err());
}

#[test]
fn orphan_review_is_rejected() {
    assert!(store::items(&[json!({"action":"review", "id":"absent", "status":"open"})]).is_err());
}
