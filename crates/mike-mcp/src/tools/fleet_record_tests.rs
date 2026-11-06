use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn arguments(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

#[tokio::test]
async fn get_fleet_record_reads_the_stored_record_without_a_query() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("GET"))
        .and(path("/api/fleets/f1/record"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"time_to_done_minutes": 353})),
        )
        .mount(&server)
        .await;
    let result = call_tool(
        &client,
        TOOL_GET_FLEET_RECORD,
        arguments(json!({"fleet": "f1"})),
    )
    .await?;
    assert_eq!(
        result.structured_content,
        Some(json!({"record": {"time_to_done_minutes": 353}}))
    );
    Ok(())
}

#[tokio::test]
async fn get_fleet_record_passes_against_and_recompute() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("GET"))
        .and(path("/api/fleets/f1/record"))
        .and(query_param("against", "f0"))
        .and(query_param("recompute", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"against": {}})))
        .mount(&server)
        .await;
    let result = call_tool(
        &client,
        TOOL_GET_FLEET_RECORD,
        arguments(json!({"fleet": "f1", "against": "f0", "recompute": true})),
    )
    .await?;
    assert_eq!(
        result.structured_content,
        Some(json!({"record": {"against": {}}}))
    );
    Ok(())
}

#[tokio::test]
async fn get_fleet_record_names_the_fleet_when_it_is_unknown() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let client = InspectClient::new(server.uri());
    Mock::given(method("GET"))
        .and(path("/api/fleets/nobody/record"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let error = call_tool(
        &client,
        TOOL_GET_FLEET_RECORD,
        arguments(json!({"fleet": "nobody"})),
    )
    .await
    .err()
    .ok_or_else(|| anyhow::anyhow!("an unknown fleet must be an error"))?;
    assert!(error.message.contains("fleet nobody"), "{}", error.message);
    Ok(())
}

#[test]
fn the_tool_takes_fleet_against_and_recompute() {
    let tool = get_fleet_record_tool();
    let properties = tool.input_schema.get("properties");
    for name in ["fleet", "against", "recompute", "select"] {
        assert!(properties.and_then(|p| p.get(name)).is_some(), "{name}");
    }
}
