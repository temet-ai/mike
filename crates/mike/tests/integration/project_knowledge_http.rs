use serde_json::{Value, json};
use uuid::Uuid;

use crate::test_db::TestDb;

use mike::config::{AppState, ProxyConfig};
use mike::store::persistence::PersistHandle;

async fn post(
    client: &reqwest::Client,
    base: &str,
    route: &str,
    body: Value,
) -> anyhow::Result<Value> {
    let response = client
        .post(format!("{base}{route}"))
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    let value: Value = response.json().await?;
    anyhow::ensure!(status.is_success(), "{route}: {status}: {value}");
    Ok(value)
}

#[tokio::test]
async fn project_knowledge_http_registration_and_full_read() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(PersistHandle::connect(&db.database_url).await?);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let router = mike::app::inspect_router_on(state.clone(), listener.local_addr()?);
    let base = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let client = reqwest::Client::new();
    let project = post(
        &client,
        &base,
        "/api/knowledge/projects",
        json!({"name":"Portable project"}),
    )
    .await?;
    let location = post(
        &client,
        &base,
        "/api/knowledge/locations",
        json!({
            "project":project["project_uuid"], "machine_uuid":Uuid::new_v4(),
            "machine_name":"Mac", "root_path":"/Users/test/app"
        }),
    )
    .await?;
    let session = Uuid::new_v4();
    let registration = post(&client, &base, "/api/workspace/register", json!({
        "session_id":session, "cwd":"/Users/test/app", "location_uuid":location["location_uuid"],
        "registered_via":"mcp", "goal":"Investigate identity"
    })).await?;
    assert_eq!(registration["project_id"], project["id"]);
    assert!(
        registration["briefing"]
            .as_str()
            .is_some_and(|s| s.contains("search_project_knowledge"))
    );
    let source = post(&client, &base, "/api/knowledge/sources", json!({
        "location_uuid":location["location_uuid"], "name":"artifacts", "root_path":"/Users/test/artifacts"
    })).await?;
    use sha2::{Digest, Sha256};
    let body = "Notebook API evidence";
    let hash: String = Sha256::digest(body.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let doc = json!({"relative_path":"design.md", "title":"Notebook design", "body":body,
        "content_hash":hash, "status":"proposed", "kind":"design", "metadata":{}});
    let route = format!(
        "/api/knowledge/sources/{}/scan",
        source["source_uuid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("source missing"))?
    );
    post(
        &client,
        &base,
        &route,
        json!({"generation":0, "documents":[doc]}),
    )
    .await?;
    let project_uuid = project["project_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("project missing"))?;
    let result: Value = client
        .get(format!("{base}/api/knowledge/search"))
        .query(&[("project", project_uuid), ("query", "notebook")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let reference = result["items"][0]["reference"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("reference missing"))?;
    let content: Value = client
        .get(format!("{base}/api/knowledge/read"))
        .query(&[("project", project_uuid), ("reference", reference)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(content["body"], "Notebook API evidence");
    assert_eq!(content["status"], "proposed");
    verify_hybrid_and_fallback(&state, &base, project_uuid).await?;
    server.abort();
    let _ = server.await;
    db.cleanup().await?;
    Ok(())
}

async fn verify_hybrid_and_fallback(
    state: &AppState,
    base: &str,
    project: &str,
) -> anyhow::Result<()> {
    use std::time::Duration;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
    let embeddings = MockServer::start().await;
    let mut vector = vec![0.0_f32; 768];
    vector[0] = 1.0;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":[{"embedding":vector}]})),
        )
        .mount(&embeddings)
        .await;
    let service = state.embedding_service();
    service.apply_setting("embedding.url", &json!(embeddings.uri()));
    service.apply_setting("embedding.enabled", &json!(true));
    let handle = state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence missing"))?;
    let mut breaker = mike::circuit_breaker::CircuitBreaker::new(
        3,
        Duration::from_secs(1),
        Duration::from_secs(10),
    );
    mike::embed_worker::knowledge::sweep(handle, &service, &mut breaker).await?;
    let client = reqwest::Client::new();
    let result = search_http(&client, base, project, "unrelated wording").await?;
    assert_eq!(result["retrieval_path"], "hybrid");
    assert_eq!(result["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(result["embedding"]["coverage"]["ready"], 1);
    embeddings.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&embeddings)
        .await;
    let result = search_http(&client, base, project, "notebook").await?;
    assert_eq!(result["retrieval_path"], "lexical");
    assert_eq!(result["embedding"]["fallback"], "embedding_unavailable");
    assert_eq!(result["items"].as_array().map(Vec::len), Some(1));
    Ok(())
}

async fn search_http(
    client: &reqwest::Client,
    base: &str,
    project: &str,
    query: &str,
) -> anyhow::Result<Value> {
    Ok(client
        .get(format!("{base}/api/knowledge/search"))
        .query(&[("project", project), ("query", query)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}
