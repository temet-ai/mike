use super::*;

fn complete() -> Value {
    json!({
        "problem_id": "0b1f0c8e-6d0e-4a52-9c2b-1d5a7b2f9a10",
        "decision": "rerun_targeted",
        "reason": "the failing test was a crash of the test database",
        "evidence_read": ["note:1"]
    })
}

fn message(error: Option<RmcpError>) -> String {
    error.map(|e| e.message.to_string()).unwrap_or_default()
}

#[test]
fn a_complete_decision_passes() {
    assert!(decision_problem(&complete()).is_none());
}

#[test]
fn a_decision_missing_a_field_is_refused_naming_it() {
    for field in ["problem_id", "decision", "reason", "evidence_read"] {
        let mut body = complete();
        if let Some(fields) = body.as_object_mut() {
            fields.remove(field);
        }
        let text = message(decision_problem(&body));
        assert!(text.contains(&format!("`{field}`")), "{field}: {text}");
    }
}

#[test]
fn an_empty_evidence_list_is_refused() {
    let mut body = complete();
    body["evidence_read"] = json!([]);
    assert!(message(decision_problem(&body)).contains("`evidence_read`"));
}

#[test]
fn a_decision_outside_the_allowed_set_is_refused_listing_the_set() {
    let mut body = complete();
    body["decision"] = json!("edit_files");
    let text = message(decision_problem(&body));
    assert!(
        text.contains("edit_files") && text.contains("record_fault"),
        "{text}"
    );
}

#[test]
fn an_escalation_needs_its_question() {
    let mut body = complete();
    body["decision"] = json!("escalate");
    assert!(message(decision_problem(&body)).contains("`question`"));
    body["question"] = json!("Keep C4 as worded?");
    assert!(decision_problem(&body).is_none());
}

#[test]
fn the_route_names_the_fleet() -> anyhow::Result<()> {
    assert_eq!(fleet_route("f1")?, "/api/fleets/f1/decide");
    Ok(())
}

#[test]
fn the_schema_requires_the_typed_fields_and_lists_every_decision() {
    let tools = fleet_decide_tools();
    let schema = serde_json::to_string(&*tools[0].input_schema).unwrap_or_default();
    for field in ["problem_id", "decision", "reason", "evidence_read"] {
        assert!(schema.contains(field), "{field}");
    }
    for decision in DECISIONS {
        assert!(schema.contains(decision), "{decision}");
    }
}
