use super::dto::{SessionFlowCursor, SessionFlowPage};

#[test]
fn session_flow_sanitized_examples_roundtrip() -> anyhow::Result<()> {
    for fixture in [
        include_str!(
            "../../../../../artifacts/2026-09-10/session-flow/contract/eligible-empty.json"
        ),
        include_str!("../../../../../artifacts/2026-09-10/session-flow/contract/concurrent.json"),
        include_str!(
            "../../../../../artifacts/2026-09-10/session-flow/contract/missing-correlation.json"
        ),
        include_str!("../../../../../artifacts/2026-09-10/session-flow/contract/unavailable.json"),
        include_str!(
            "../../../../../artifacts/2026-09-10/session-flow/contract/partial-retention.json"
        ),
        include_str!("../../../../../artifacts/2026-09-10/session-flow/contract/agent-name.json"),
        include_str!(
            "../../../../../artifacts/2026-09-10/session-flow/contract/dispatch-description.json"
        ),
        include_str!(
            "../../../../../artifacts/2026-09-10/session-flow/contract/firing-rule-count.json"
        ),
    ] {
        let value: serde_json::Value = serde_json::from_str(fixture)?;
        let page: SessionFlowPage = serde_json::from_value(value.clone())?;
        assert_eq!(serde_json::to_value(page)?, value);
    }
    Ok(())
}

#[test]
fn session_flow_cursor_preserves_large_source_ids() -> anyhow::Result<()> {
    let page: SessionFlowPage = serde_json::from_str(include_str!(
        "../../../../../artifacts/2026-09-10/session-flow/contract/partial-retention.json"
    ))?;
    let cursor = page
        .next_cursor
        .ok_or_else(|| anyhow::anyhow!("missing cursor"))?;
    let parsed: SessionFlowCursor = serde_json::from_str(&cursor)?;
    assert_eq!(parsed.after_id, "9007199254740993");
    assert_eq!(parsed.upper_id, "9007199254740995");
    Ok(())
}

#[test]
fn session_flow_rejects_raw_fields() -> anyhow::Result<()> {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../artifacts/2026-09-10/session-flow/contract/eligible-empty.json"
    ))?;
    value["body"] = serde_json::json!("must never enter bulk data");
    assert!(serde_json::from_value::<SessionFlowPage>(value).is_err());
    Ok(())
}

#[test]
fn session_flow_cursor_rejects_wrong_version_scope_and_bounds() -> anyhow::Result<()> {
    let page: SessionFlowPage = serde_json::from_str(include_str!(
        "../../../../../artifacts/2026-09-10/session-flow/contract/partial-retention.json"
    ))?;
    let raw = page
        .next_cursor
        .ok_or_else(|| anyhow::anyhow!("missing cursor"))?;
    let cursor: SessionFlowCursor = serde_json::from_str(&raw)?;
    assert!(cursor.matches_scope(&page.session_id, &page.coverage.component));
    assert!(!cursor.matches_scope("other-session", &page.coverage.component));
    assert!(!cursor.matches_scope(&page.session_id, &super::dto::SessionFlowComponent::Tools));
    for (field, value) in [
        ("version", serde_json::json!(2)),
        ("filter", serde_json::json!("body")),
        ("after_id", serde_json::json!("-1")),
        ("after_id", serde_json::json!("9007199254740996")),
        ("upper_id", serde_json::json!("9223372036854775808")),
    ] {
        let mut value_json = serde_json::to_value(&cursor)?;
        value_json[field] = value;
        let invalid: SessionFlowCursor = serde_json::from_value(value_json)?;
        assert!(!invalid.matches_scope(&page.session_id, &page.coverage.component));
    }
    Ok(())
}
