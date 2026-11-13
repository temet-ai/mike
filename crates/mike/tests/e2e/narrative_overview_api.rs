use crate::db_helpers;
use crate::seed;

use mike::app::inspect_router_on;
use mike::config::ProxyConfig;
use mike::state::AppState;
use seed::{seed_project, seed_session};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn or_overview_response(overview: &str) -> Value {
    let content = json!({ "overview": overview }).to_string();
    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop"
        }]
    })
}

async fn start_inspect_with_llm(
    upstream_url: &str,
    database_url: &str,
    or_base_url: &str,
) -> anyhow::Result<(String, CancellationToken)> {
    let mut config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
    ])?;
    config.set_openrouter_base_url(or_base_url.to_owned());
    config.set_openrouter_api_key("test-api-key".to_owned());

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state, inspect_listener.local_addr()?);

    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });

    Ok((format!("http://{inspect_addr}"), shutdown))
}

async fn read_overview_columns(
    pool: &sqlx::PgPool,
    session_uuid: Uuid,
) -> anyhow::Result<Option<(Option<String>, Option<chrono::DateTime<chrono::Utc>>)>> {
    Ok(sqlx::query_as(
        "SELECT overview, overview_generated_at
         FROM sessions.sessions WHERE session_uuid = $1",
    )
    .bind(session_uuid)
    .fetch_optional(pool)
    .await?)
}

#[tokio::test]
async fn patch_empty_overview_rejected_with_400() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };

    let project_id = seed_project(&proxy.pool, "/proj/narrative-patch-400").await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&proxy.pool, project_id, &session_uuid.to_string()).await?;

    let client = reqwest::Client::new();
    let resp = client
        .patch(format!(
            "{}/api/sessions/{session_uuid}/narrative-overview",
            proxy.inspect_url
        ))
        .json(&json!({ "overview": "   " }))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        400,
        "whitespace-only overview must be rejected"
    );

    let row = read_overview_columns(&proxy.pool, session_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("seeded session row must exist"))?;
    assert_eq!(row.0, None, "rejected PATCH must not write the overview");

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn patch_unknown_session_rejected_with_404() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };

    let client = reqwest::Client::new();

    let resp = client
        .patch(format!(
            "{}/api/sessions/{}/narrative-overview",
            proxy.inspect_url,
            Uuid::new_v4()
        ))
        .json(&json!({ "overview": "orphan overview" }))
        .send()
        .await?;
    assert_eq!(resp.status(), 404, "unknown session must 404");

    let resp = client
        .patch(format!(
            "{}/api/sessions/not-a-uuid/narrative-overview",
            proxy.inspect_url
        ))
        .json(&json!({ "overview": "orphan overview" }))
        .send()
        .await?;
    assert_eq!(resp.status(), 404, "non-UUID session id must 404");

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn patch_persists_operator_overview_and_surfaces_on_get() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };

    let project_id = seed_project(&proxy.pool, "/proj/narrative-patch-200").await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&proxy.pool, project_id, &session_uuid.to_string()).await?;

    let overview_text = "The session set out to rework the session page layout.\n\n\
        It landed the mailbox move, the overview card, and the timeline rows.";

    let client = reqwest::Client::new();
    let resp = client
        .patch(format!(
            "{}/api/sessions/{session_uuid}/narrative-overview",
            proxy.inspect_url
        ))
        .json(&json!({ "overview": overview_text }))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["overview"], json!(overview_text));
    assert_eq!(
        body["generated_at"],
        Value::Null,
        "operator-authored PATCH must report generated_at = null"
    );

    let row = read_overview_columns(&proxy.pool, session_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("seeded session row must exist"))?;
    assert_eq!(row.0.as_deref(), Some(overview_text));
    assert_eq!(
        row.1, None,
        "operator-authored set must leave generated_at NULL"
    );

    let resp = client
        .get(format!("{}/api/sessions/{session_uuid}", proxy.inspect_url))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let detail: Value = resp.json().await?;
    let summary = detail
        .get("summary")
        .unwrap_or(&detail)
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("session detail must be a JSON object"))?;
    assert_eq!(
        summary.get("narrative_overview"),
        Some(&json!(overview_text)),
        "narrative_overview must surface the full text on GET /api/sessions/{{id}}"
    );
    assert_eq!(
        summary.get("narrative_overview_generated_at"),
        Some(&Value::Null),
    );

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn session_overview_endpoint_carries_narrative_overview() -> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };

    let project_id = seed_project(&proxy.pool, "/proj/narrative-on-overview").await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&proxy.pool, project_id, &session_uuid.to_string()).await?;

    let client = reqwest::Client::new();

    let before: Value = client
        .get(format!(
            "{}/api/sessions/{session_uuid}/overview",
            proxy.inspect_url
        ))
        .send()
        .await?
        .json()
        .await?;
    assert!(
        before.get("narrative_overview").is_none(),
        "narrative_overview must be absent until one exists, got {:?}",
        before.get("narrative_overview")
    );

    let overview_text = "Locating where consolidation runs are recorded.\n\n\
        The lookup landed on memory.consolidation_runs; two call sites still read the old view.";
    let resp = client
        .patch(format!(
            "{}/api/sessions/{session_uuid}/narrative-overview",
            proxy.inspect_url
        ))
        .json(&json!({ "overview": overview_text }))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let after: Value = client
        .get(format!(
            "{}/api/sessions/{session_uuid}/overview",
            proxy.inspect_url
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        after.get("narrative_overview"),
        Some(&json!(overview_text)),
        "GET /api/sessions/{{id}}/overview must carry the full narrative text"
    );
    assert!(
        after.get("narrative_overview_generated_at").is_none(),
        "operator-authored overview leaves generated_at absent on this endpoint"
    );
    assert!(after.get("turns").is_some(), "turns must still be present");
    assert!(
        after.get("total_turns").is_some(),
        "total_turns must still be present"
    );

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn generate_persists_llm_overview_with_timestamp() -> anyhow::Result<()> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let upstream = MockServer::start().await;
    let or_mock = MockServer::start().await;
    let generated = "The session set out to add narrative overviews.\n\n\
        It landed the schema, the store slice, and both endpoints.";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(or_overview_response(generated)))
        .expect(1)
        .mount(&or_mock)
        .await;

    let (inspect_url, shutdown) =
        start_inspect_with_llm(&upstream.uri(), &test_db.database_url, &or_mock.uri()).await?;

    let project_id = seed_project(&pool, "/proj/narrative-generate").await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&pool, project_id, &session_uuid.to_string()).await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{inspect_url}/api/sessions/{session_uuid}/narrative-overview/generate"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["overview"], json!(generated));
    assert!(
        body["generated_at"].is_string(),
        "LLM generation must report a generated_at timestamp"
    );

    let row = read_overview_columns(&pool, session_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("seeded session row must exist"))?;
    assert_eq!(row.0.as_deref(), Some(generated));
    assert!(
        row.1.is_some(),
        "LLM-generated set must stamp overview_generated_at"
    );

    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn generate_unknown_session_404_without_llm_call() -> anyhow::Result<()> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;

    let upstream = MockServer::start().await;
    let or_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(or_overview_response("unused")))
        .expect(0)
        .mount(&or_mock)
        .await;

    let (inspect_url, shutdown) =
        start_inspect_with_llm(&upstream.uri(), &test_db.database_url, &or_mock.uri()).await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{inspect_url}/api/sessions/{}/narrative-overview/generate",
            Uuid::new_v4()
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        404,
        "unknown session must 404 before the LLM call"
    );

    shutdown.cancel();
    Ok(())
}
