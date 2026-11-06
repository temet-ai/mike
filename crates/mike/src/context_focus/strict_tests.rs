use serde_json::Value;

use super::FIELD;
use super::inject::{InjectOutcome, Mode, inject};
use super::strict::{MAX_OPTIONAL_PARAMETERS, optional_parameters, sanitise};

const BODY: &[u8] = br#"{"model":"m","messages":[],"tools":[{"name":"Grep","input_schema":{"type":"object","properties":{"pattern":{"type":"string"},"head_limit":{"type":"integer","minimum":0},"path":{"type":"string"}},"required":["pattern"]}},{"name":"Read","input_schema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"properties":{"file_path":{"type":"string"},"limit":{"type":"integer","exclusiveMinimum":0,"maximum":9007199254740991},"pages":{"type":"string","maxLength":20}},"required":["file_path"]}},{"name":"Edit","input_schema":{"type":"object","properties":{"old_string":{"type":"string"}}}},{"name":"Later","defer_loading":true,"input_schema":{"type":"object","properties":{}}}]}"#;

fn strict_body(raw: &[u8]) -> (Value, Vec<String>, Vec<String>) {
    match inject(raw, Mode::Strict) {
        InjectOutcome::Injected {
            new_body,
            tools,
            strict_tools,
        } => (
            serde_json::from_slice(&new_body).unwrap_or(Value::Null),
            tools,
            strict_tools,
        ),
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
fn strict_mode_marks_extended_tools_strict_and_skips_edit_and_deferred() {
    let (body, tools, strict_tools) = strict_body(BODY);
    assert_eq!(tools, vec!["Grep", "Read", "Later"]);
    assert_eq!(
        strict_tools,
        vec!["Read"],
        "fourteen strict tools exceed the API's compiled-grammar limit; Read is the one compression needs"
    );
    assert_eq!(tool(&body, "Read")["strict"], true);
    assert!(tool(&body, "Grep").get("strict").is_none());
    assert!(tool(&body, "Edit").get("strict").is_none());
    assert!(tool(&body, "Later").get("strict").is_none());
}

#[test]
fn strict_mode_removes_constraints_the_api_rejects_and_closes_objects() {
    let (body, _, _) = strict_body(BODY);
    let read = &tool(&body, "Read")["input_schema"];
    assert_eq!(read["additionalProperties"], false);
    assert!(
        read["properties"]["limit"]
            .get("exclusiveMinimum")
            .is_none()
    );
    assert!(read["properties"]["limit"].get("maximum").is_none());
    assert!(read["properties"]["pages"].get("maxLength").is_none());
    assert!(read["properties"][FIELD].get("maxLength").is_none());
}

#[test]
fn sanitise_keeps_a_property_named_like_a_keyword() {
    let mut schema = serde_json::json!({
        "type": "object",
        "properties": {"pattern": {"type": "string"}, "maxLength": {"type": "integer"}}
    });
    sanitise(&mut schema);
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("pattern").is_some());
    assert!(schema["properties"].get("maxLength").is_some());
}

#[test]
fn strict_mode_same_body_twice_gives_identical_bytes() {
    let first = match inject(BODY, Mode::Strict) {
        InjectOutcome::Injected { new_body, .. } => new_body,
        InjectOutcome::Unchanged => panic!("expected the field to be added"),
    };
    let second = match inject(BODY, Mode::Strict) {
        InjectOutcome::Injected { new_body, .. } => new_body,
        InjectOutcome::Unchanged => panic!("expected the field to be added"),
    };
    assert_eq!(first, second);
}

#[test]
fn strict_mode_keeps_optional_parameters_within_the_api_budget_and_prefers_read() {
    let mut tools = String::new();
    for i in 0..10 {
        let props: Vec<String> = (0..5)
            .map(|p| format!(r#""p{p}":{{"type":"string"}}"#))
            .collect();
        tools += &format!(
            r#"{{"name":"T{i}","input_schema":{{"type":"object","properties":{{{}}}}}}},"#,
            props.join(",")
        );
    }
    tools += r#"{"name":"Read","input_schema":{"type":"object","properties":{"file_path":{"type":"string"},"limit":{"type":"integer"}},"required":["file_path"]}}"#;
    let raw = format!(r#"{{"messages":[],"tools":[{tools}]}}"#);
    let (body, _, strict_tools) = strict_body(raw.as_bytes());
    assert!(strict_tools.contains(&"Read".to_owned()));
    let optional: usize = strict_tools
        .iter()
        .map(|name| optional_parameters(tool(&body, name)))
        .sum();
    assert!(optional <= MAX_OPTIONAL_PARAMETERS, "{optional} optional");
}

#[test]
fn plain_and_emphatic_modes_mark_nothing_strict() {
    for mode in [Mode::Plain, Mode::Emphatic] {
        let InjectOutcome::Injected { strict_tools, .. } = inject(BODY, mode) else {
            panic!("expected the field to be added");
        };
        assert!(strict_tools.is_empty());
    }
}

#[test]
fn mode_parse_unknown_setting_falls_back_to_plain() {
    assert_eq!(Mode::parse("strict"), Mode::Strict);
    assert_eq!(Mode::parse("emphatic"), Mode::Emphatic);
    assert_eq!(Mode::parse("bogus"), Mode::Plain);
}

#[test]
fn sanitise_non_object_value_is_left_alone() {
    let mut value = Value::String("x".to_owned());
    sanitise(&mut value);
    assert_eq!(value, "x");
}
