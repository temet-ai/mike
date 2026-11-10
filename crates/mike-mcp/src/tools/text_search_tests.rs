use serde_json::{Map, Value, json};

use super::{SearchSessionContentArgs, search_session_content_query, search_session_content_tool};

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
