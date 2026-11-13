use serde_json::Value;

use super::FIELD;
use super::inject::{InjectOutcome, Mode, inject};

const BODY: &[u8] = br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":"S","tools":[{"name":"Read","description":"r","input_schema":{"type":"object","properties":{"file_path":{"type":"string"}},"required":["file_path"]}},{"name":"Edit","input_schema":{"type":"object","properties":{"old_string":{"type":"string"}}}},{"name":"Glob","input_schema":{"type":"object","properties":{"pattern":{"type":"string"}}}},{"name":"mike_session_roster","input_schema":{"type":"object","properties":{}}},{"type":"web_search_20250305","name":"web_search"}],"max_tokens":16}"#;

fn injected(raw: &[u8]) -> (Vec<u8>, Vec<String>) {
    match inject(raw, Mode::Plain) {
        InjectOutcome::Injected {
            new_body, tools, ..
        } => (new_body.to_vec(), tools),
        InjectOutcome::Unchanged => panic!("expected the field to be added"),
    }
}

fn tool<'a>(body: &'a Value, name: &str) -> &'a Value {
    body["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|t| t["name"] == name))
        .unwrap_or(&Value::Null)
}

#[test]
fn inject_client_tools_adds_a_required_field_to_each() -> anyhow::Result<()> {
    let (bytes, tools) = injected(BODY);
    assert_eq!(tools, vec!["Read", "Glob"]);
    let body: Value = serde_json::from_slice(&bytes)?;
    let read = tool(&body, "Read");
    assert_eq!(read["input_schema"]["properties"][FIELD]["maxLength"], 200);
    assert_eq!(
        read["input_schema"]["required"],
        serde_json::json!(["file_path", FIELD])
    );
    assert_eq!(
        tool(&body, "Glob")["input_schema"]["required"],
        serde_json::json!([FIELD])
    );
    Ok(())
}

#[test]
fn inject_file_editing_proxy_and_server_tools_leaves_them_alone() -> anyhow::Result<()> {
    let (bytes, _) = injected(BODY);
    let body: Value = serde_json::from_slice(&bytes)?;
    assert!(tool(&body, "Edit")["input_schema"]["properties"][FIELD].is_null());
    assert!(
        tool(&body, crate::proxy_tools::roster::TOOL_NAME)["input_schema"]["properties"][FIELD]
            .is_null()
    );
    assert!(tool(&body, "web_search").get("input_schema").is_none());
    Ok(())
}

#[test]
fn inject_same_body_twice_gives_identical_bytes() {
    assert_eq!(
        injected(BODY).0,
        injected(BODY).0,
        "a schema edit that varies between requests misses the prompt cache every turn"
    );
}

#[test]
fn inject_keeps_every_other_top_level_key_byte_identical() -> anyhow::Result<()> {
    let after = String::from_utf8(injected(BODY).0)?;
    assert!(after.starts_with(
        r#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":"S","tools":["#
    ));
    assert!(after.ends_with(r#"],"max_tokens":16}"#));
    Ok(())
}

#[test]
fn inject_already_injected_body_is_unchanged() {
    let (bytes, _) = injected(BODY);
    assert!(matches!(
        inject(&bytes, Mode::Plain),
        InjectOutcome::Unchanged
    ));
}

#[test]
fn inject_body_without_tools_is_unchanged() {
    assert!(matches!(
        inject(br#"{"model":"m","messages":[],"tools":[]}"#, Mode::Plain),
        InjectOutcome::Unchanged
    ));
    assert!(matches!(
        inject(br#"{"model":"m","messages":[]}"#, Mode::Plain),
        InjectOutcome::Unchanged
    ));
    assert!(matches!(
        inject(b"not json", Mode::Plain),
        InjectOutcome::Unchanged
    ));
}
