use super::InspectClient;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path, query_param},
};

#[tokio::test]
async fn knowledge_search_preserves_project_scope_and_filters() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/knowledge/search"))
        .and(query_param("project", "project-uuid"))
        .and(query_param("query", "notebook identity"))
        .and(query_param("include_historical", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]})))
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    let args =
        json!({"project":"project-uuid", "query":"notebook identity", "include_historical":true});
    let args = args
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("object required"))?;
    assert_eq!(
        client
            .knowledge_request("search_project_knowledge", args)
            .await?["items"],
        json!([])
    );
    Ok(())
}

#[tokio::test]
async fn knowledge_location_registration_forwards_machine_identity() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let args = json!({"project":"p", "machine_uuid":"m", "machine_name":"Mac", "root_path":"/Users/dev/app"});
    Mock::given(method("POST"))
        .and(path("/api/knowledge/locations"))
        .and(body_json(&args))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({"error":"Location belongs to another project"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = InspectClient::new(server.uri());
    assert!(
        client
            .knowledge_request(
                "register_project_location",
                args.as_object()
                    .ok_or_else(|| anyhow::anyhow!("object required"))?
            )
            .await
            .is_err()
    );
    Ok(())
}
