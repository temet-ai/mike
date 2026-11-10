use serde_json::{Map, Value, json};

use super::super::text_search_find::{FindTextArgs, find_text_query};
use super::{
    SearchSessionContentArgs, find_text_tool, search_session_content_query,
    search_session_content_tool,
};

#[test]
fn find_text_query_passes_the_caller_session_under_its_own_key() -> anyhow::Result<()> {
    let args: FindTextArgs = serde_json::from_value(json!({
        "needle": "abc",
        "session_id": "scope",
        "caller_session_id": "caller"
    }))?;

    let query = find_text_query(&args);

    assert!(query.contains(&("session_id", "scope".to_owned())));
    assert!(query.contains(&("caller_session_id", "caller".to_owned())));
    Ok(())
}

#[test]
fn search_session_content_query_passes_the_caller_session() -> anyhow::Result<()> {
    let args: SearchSessionContentArgs =
        serde_json::from_value(json!({"q": "abc", "caller_session_id": "caller"}))?;

    let query = search_session_content_query(&args);

    assert!(query.contains(&("caller_session_id", "caller".to_owned())));
    Ok(())
}

fn arguments(value: Value) -> anyhow::Result<Map<String, Value>> {
    match value {
        Value::Object(map) => Ok(map),
        other => anyhow::bail!("arguments must be an object, got {other}"),
    }
}

#[test]
fn search_session_content_schema_declares_order_with_both_values() -> anyhow::Result<()> {
    let tool = search_session_content_tool();
    let order = tool
        .input_schema
        .get("properties")
        .and_then(|properties| properties.get("order"))
        .ok_or_else(|| anyhow::anyhow!("`order` is not in the schema"))?;

    assert_eq!(order["enum"], json!(["matches", "recent"]));
    Ok(())
}

#[test]
fn find_text_schema_declares_whole_bodies_and_include_reminders_as_booleans() -> anyhow::Result<()>
{
    let tool = find_text_tool();
    let properties = tool
        .input_schema
        .get("properties")
        .ok_or_else(|| anyhow::anyhow!("no properties in the schema"))?;

    assert_eq!(properties["whole_bodies"]["type"], json!("boolean"));
    assert_eq!(properties["include_reminders"]["type"], json!("boolean"));
    Ok(())
}

#[test]
fn find_text_query_passes_whole_bodies_and_include_reminders_through() -> anyhow::Result<()> {
    let args: FindTextArgs = serde_json::from_value(
        json!({"needle": "abc", "whole_bodies": true, "include_reminders": false}),
    )?;

    let query = find_text_query(&args);

    assert!(
        query.contains(&("whole_bodies", "true".to_owned())),
        "{query:?}"
    );
    assert!(
        query.contains(&("include_reminders", "false".to_owned())),
        "{query:?}"
    );
    Ok(())
}

#[test]
fn find_text_query_without_the_new_arguments_leaves_defaults_to_the_server() -> anyhow::Result<()> {
    let args: FindTextArgs = serde_json::from_value(json!({"needle": "abc"}))?;

    let query = find_text_query(&args);

    assert!(
        query
            .iter()
            .all(|(name, _)| *name != "whole_bodies" && *name != "include_reminders"),
        "{query:?}"
    );
    Ok(())
}

#[test]
fn search_session_content_query_passes_order_through() -> anyhow::Result<()> {
    let args: SearchSessionContentArgs = serde_json::from_value(Value::Object(arguments(
        json!({"q": "workflow", "project": "radar", "order": "recent"}),
    )?))?;

    let query = search_session_content_query(&args);

    assert!(query.contains(&("order", "recent".to_owned())), "{query:?}");
    assert!(
        query.contains(&("project", "radar".to_owned())),
        "{query:?}"
    );
    Ok(())
}

#[test]
fn search_session_content_query_without_order_leaves_the_default_to_the_server()
-> anyhow::Result<()> {
    let args: SearchSessionContentArgs = serde_json::from_value(json!({"q": "workflow"}))?;

    let query = search_session_content_query(&args);

    assert!(query.iter().all(|(name, _)| *name != "order"), "{query:?}");
    Ok(())
}
