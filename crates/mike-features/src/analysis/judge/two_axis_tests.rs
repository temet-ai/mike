use super::two_axis::{Delivery, Reaction, TwoAxisVerdict, two_axis_verdict_schema};

#[test]
fn the_schema_is_strict_and_matches_the_w4_candidate() {
    let schema = two_axis_verdict_schema();
    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    assert_eq!(schema["required"].as_array().map(Vec::len), Some(6));
    assert!(schema["properties"].get("turn_id").is_none());
}

#[test]
fn a_verdict_parses_without_tags_or_severity() -> anyhow::Result<()> {
    let verdict: TwoAxisVerdict = serde_json::from_value(serde_json::json!({
        "reaction": "no_response", "delivery": "not_applicable", "confidence": 0.9,
        "rationale": "r", "unmet_ask": "", "evidence_quality": "absent"
    }))?;
    assert_eq!(verdict.reaction, Reaction::NoResponse);
    assert_eq!(verdict.delivery, Delivery::NotApplicable);
    assert!(verdict.failure_tags.is_none() && verdict.severity.is_none());
    Ok(())
}
