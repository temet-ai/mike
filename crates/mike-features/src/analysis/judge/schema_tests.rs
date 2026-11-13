use super::*;

#[test]
fn schema_is_valid_json_object() {
    let s = judge_verdict_schema();
    assert!(s.is_object(), "schema must be a JSON object");
    assert!(
        s["properties"]["label"].is_object(),
        "schema must have a label property"
    );
    assert!(
        s["properties"]["confidence"].is_object(),
        "schema must have a confidence property"
    );
    assert!(
        s["properties"]["rationale"].is_object(),
        "schema must have a rationale property"
    );
    assert!(
        s["properties"]["unmet_ask"].is_object(),
        "schema must have an unmet_ask property"
    );
    assert!(
        s["properties"]["evidence_quality"].is_object(),
        "schema must have an evidence_quality property"
    );
}

#[test]
fn schema_label_has_all_enum_values() -> anyhow::Result<()> {
    let s = judge_verdict_schema();
    let enums = s["properties"]["label"]["enum"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("label must have an enum array"))?;
    let values: Vec<&str> = enums.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        values.contains(&"accepted"),
        "schema must include 'accepted'"
    );
    assert!(values.contains(&"shipped"), "schema must include 'shipped'");
    assert!(
        values.contains(&"corrected"),
        "schema must include 'corrected'"
    );
    assert!(
        values.contains(&"abandoned"),
        "schema must include 'abandoned'"
    );
    assert!(values.contains(&"unknown"), "schema must include 'unknown'");
    Ok(())
}

#[test]
fn schema_has_required_fields() -> anyhow::Result<()> {
    let s = judge_verdict_schema();
    let required = s["required"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("required must be array"))?;
    let req_strs: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
    assert!(req_strs.contains(&"label"));
    assert!(req_strs.contains(&"confidence"));
    assert!(
        req_strs.contains(&"rationale"),
        "rationale must be required — with strict structured output, an \
         optional field is a field the model will often omit"
    );
    assert!(
        req_strs.contains(&"unmet_ask"),
        "unmet_ask must be required"
    );
    assert!(
        req_strs.contains(&"evidence_quality"),
        "evidence_quality must be required"
    );
    Ok(())
}

#[test]
fn schema_evidence_quality_has_exactly_three_enum_values() -> anyhow::Result<()> {
    let s = judge_verdict_schema();
    let enums = s["properties"]["evidence_quality"]["enum"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("evidence_quality must have an enum array"))?;
    let values: Vec<&str> = enums.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(
        values,
        vec!["verified", "asserted", "absent"],
        "evidence_quality must carry exactly these three values, got {values:?}"
    );
    Ok(())
}

#[test]
fn schema_additional_properties_false() {
    let s = judge_verdict_schema();
    assert_eq!(
        s["additionalProperties"].as_bool(),
        Some(false),
        "additionalProperties must be false"
    );
}

#[test]
fn each_label_maps_to_correct_outcome_code() {
    assert_eq!(JudgeLabel::Accepted.outcome_code(), "accepted");
    assert_eq!(JudgeLabel::Shipped.outcome_code(), "shipped");
    assert_eq!(JudgeLabel::Corrected.outcome_code(), "corrected");
    assert_eq!(JudgeLabel::Abandoned.outcome_code(), "abandoned");
    assert_eq!(JudgeLabel::Unknown.outcome_code(), "unknown");
}

#[test]
fn positive_labels_exist() {
    let accepted = JudgeLabel::Accepted;
    let shipped = JudgeLabel::Shipped;
    assert_eq!(accepted.outcome_code(), "accepted");
    assert_eq!(shipped.outcome_code(), "shipped");
}

#[test]
fn verdict_deserializes_accepted() -> anyhow::Result<()> {
    let v: JudgeVerdict = serde_json::from_str(
        r#"{"label": "accepted", "confidence": 0.88, "rationale": "delivered the fix", "unmet_ask": "", "evidence_quality": "verified"}"#,
    )?;
    assert_eq!(v.label, JudgeLabel::Accepted);
    assert!((v.confidence - 0.88).abs() < 0.001);
    Ok(())
}

#[test]
fn verdict_deserializes_shipped() -> anyhow::Result<()> {
    let v: JudgeVerdict = serde_json::from_str(
        r#"{"label": "shipped", "confidence": 0.95, "rationale": "committed and pushed", "unmet_ask": "", "evidence_quality": "verified"}"#,
    )?;
    assert_eq!(v.label, JudgeLabel::Shipped);
    Ok(())
}

#[test]
fn verdict_deserializes_corrected() -> anyhow::Result<()> {
    let v: JudgeVerdict = serde_json::from_str(
        r#"{"label": "corrected", "confidence": 0.71, "rationale": "operator flagged the fix as still broken", "unmet_ask": "a working fix", "evidence_quality": "asserted"}"#,
    )?;
    assert_eq!(v.label, JudgeLabel::Corrected);
    Ok(())
}

#[test]
fn verdict_deserializes_abandoned() -> anyhow::Result<()> {
    let v: JudgeVerdict = serde_json::from_str(
        r#"{"label": "abandoned", "confidence": 0.6, "rationale": "turn ended with a plan and no follow-through", "unmet_ask": "the implementation", "evidence_quality": "absent"}"#,
    )?;
    assert_eq!(v.label, JudgeLabel::Abandoned);
    Ok(())
}

#[test]
fn verdict_deserializes_unknown() -> anyhow::Result<()> {
    let v: JudgeVerdict = serde_json::from_str(
        r#"{"label": "unknown", "confidence": 0.3, "rationale": "text does not say what happened", "unmet_ask": "", "evidence_quality": "absent"}"#,
    )?;
    assert_eq!(v.label, JudgeLabel::Unknown);
    Ok(())
}

#[test]
fn system_prompt_is_non_empty() {
    let p = judge_system_prompt();
    assert!(!p.is_empty());
}

#[test]
fn system_prompt_names_all_labels() {
    let p = judge_system_prompt();
    for label in &["accepted", "shipped", "corrected", "abandoned", "unknown"] {
        assert!(p.contains(label), "system prompt must name label '{label}'");
    }
}

#[test]
fn system_prompt_never_makes_accepted_the_default() {
    let p = judge_system_prompt().to_lowercase();
    assert!(
        !p.contains("positive default"),
        "rubric must not call 'accepted' a/the positive default — checked as the \
         bare phrase, not just the original \"the POSITIVE default\" wording, so a \
         later edit that reintroduces the idea with different surrounding words \
         still trips this"
    );
    assert!(
        !p.contains("choose accepted"),
        "rubric must not tell the judge to choose accepted when in doubt"
    );
    assert!(
        !p.contains("lean toward accepted"),
        "rubric must not tell the judge to lean toward accepted"
    );
}

#[test]
fn rubric_version_is_non_empty() {
    assert!(!JUDGE_RUBRIC_VERSION.is_empty());
}
