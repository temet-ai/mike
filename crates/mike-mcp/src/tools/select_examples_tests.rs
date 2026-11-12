use std::collections::HashSet;

use rmcp::model::Tool;
use serde_json::{Map, Value, json};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{EXAMPLES, select_example};
use crate::client::InspectClient;
use crate::tools::{all_tools, call_tool};

const ENVELOPE_EXTRAS: [&str; 2] = ["window_note", "next_cursor"];

fn extra_arguments(tool: &str) -> Map<String, Value> {
    let extra = match tool {
        "get_request_body" => json!({"id": 1}),
        "write_session_note" | "list_session_notes" => json!({"session_id": "x"}),
        "get_prompt_composition" => json!({"id": "x"}),
        "list_supplied_items" => json!({"request_id": 1}),
        "define_fleet" => json!({"slug": "x", "project_path": "x"}),
        "fleet_report" => json!({"exercised": {"command": "x", "exit": 0, "output_ref": "x"}}),
        "fleet_decide" => json!({"evidence_read": ["x"]}),
        "send_session_message" => json!({"to": "x"}),
        "ask" | "answer" | "decline" => json!({"from": "x"}),
        "work_thread_list"
        | "work_thread_get"
        | "work_thread_checkpoint"
        | "work_thread_add_entry" => json!({"project": "x"}),
        _ => json!({}),
    };
    extra.as_object().cloned().unwrap_or_default()
}

fn root_of(example: &str) -> &str {
    example
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or_default()
}

fn sample_value(schema: &Value) -> Value {
    if let Some(first) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
    {
        return first.clone();
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("integer") | Some("number") => json!(1),
        Some("boolean") => json!(true),
        Some("array") => json!([]),
        Some("object") => json!({}),
        _ => json!("x"),
    }
}

fn sample_arguments(tool: &Tool) -> Map<String, Value> {
    let properties = tool
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    tool.input_schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|name| {
            let schema = properties.get(name).cloned().unwrap_or(Value::Null);
            (name.to_string(), sample_value(&schema))
        })
        .collect()
}

fn envelope_key(structured: &Value) -> Option<String> {
    let keys: Vec<&String> = structured
        .as_object()?
        .keys()
        .filter(|key| !ENVELOPE_EXTRAS.contains(&key.as_str()))
        .collect();
    match keys.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    }
}

#[test]
fn every_tool_has_an_example_and_every_example_names_a_tool() {
    let names: HashSet<String> = all_tools()
        .iter()
        .map(|tool| tool.name.to_string())
        .collect();
    for name in &names {
        assert!(
            select_example(name).is_some(),
            "no select example for {name}"
        );
    }
    let mut seen = HashSet::new();
    for (tool, example) in EXAMPLES {
        assert!(names.contains(*tool), "example for unknown tool {tool}");
        assert!(seen.insert(*tool), "duplicate example for {tool}");
        assert!(
            example.starts_with('/') && !root_of(example).is_empty() && root_of(example) != "*",
            "{tool}: {example:?} must start with the tool's result key"
        );
    }
}

#[test]
fn every_select_description_shows_its_own_tools_example() -> anyhow::Result<()> {
    for tool in all_tools() {
        let example = select_example(tool.name.as_ref())
            .ok_or_else(|| anyhow::anyhow!("no example for {}", tool.name))?;
        let description = tool
            .input_schema
            .get("properties")
            .and_then(|properties| properties.get("select"))
            .and_then(|select| select.get("description"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("{} has no select description", tool.name))?;
        assert!(
            description.contains(&format!("[\"{example}\"]")),
            "{}: {description}",
            tool.name
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_example_starts_at_the_key_its_tool_wraps_the_result_in() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    let mut unprobed = Vec::new();
    for tool in all_tools() {
        let name = tool.name.to_string();
        let mut arguments = sample_arguments(&tool);
        arguments.extend(extra_arguments(&name));
        let result = call_tool(&client, &name, Some(arguments)).await;
        let key = match &result {
            Ok(result) => result.structured_content.as_ref().and_then(envelope_key),
            Err(error) => {
                unprobed.push(format!("{name}: {}", error.message));
                continue;
            }
        };
        let Some(key) = key else {
            unprobed.push(format!("{name}: no single envelope key"));
            continue;
        };
        let example =
            select_example(&name).ok_or_else(|| anyhow::anyhow!("no example for {name}"))?;
        assert_eq!(root_of(example), key, "{name}: example {example:?}");
    }
    assert!(unprobed.is_empty(), "{unprobed:#?}");
    Ok(())
}
