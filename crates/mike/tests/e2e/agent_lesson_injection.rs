use crate::anthropic_fixtures;
use crate::test_db;

use std::net::SocketAddr;

use anthropic_fixtures::{MockResponse, RequestBody, text_block};
use figment::providers::Format as _;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use test_db::TestDb;

async fn boot_proxy(
    upstream_url: &str,
    database_url: &str,
) -> anyhow::Result<(AppState, String, String, CancellationToken)> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        format!(
            "persist_enabled = true\ndatabase_url = \"{database_url}\"\n\n[lesson_worker]\nenabled = false\n"
        ),
    )?;

    mike::feature_settings::install();
    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("agent-lesson-injection proxy config extract: {e}"))?;
    config.proxy_listen_addr = "127.0.0.1:0".to_string();
    config.inspect_listen_addr = "127.0.0.1:0".to_string();
    config.upstream_url = upstream_url.to_string();

    let state = AppState::new(config).await?;
    mike::lesson_delivery::hooks::install(&state);
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr: SocketAddr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr: SocketAddr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);

    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });
    tokio::task::yield_now().await;

    Ok((
        state,
        format!("http://{proxy_addr}"),
        format!("http://{inspect_addr}"),
        shutdown,
    ))
}

fn ok_response() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(MockResponse::new("msg_agent_lesson").usage(1, 1).build())
        .insert_header("content-type", "application/json")
}

fn subagent_body(persona: &str, user_text: &str) -> Value {
    RequestBody::default()
        .max_tokens(64)
        .system(json!([text_block(format!(
            "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a {persona}, an expert who reviews code for correctness.\n"
        ))]))
        .content(json!([text_block(user_text)]))
        .build()
}

fn main_session_body(user_text: &str) -> Value {
    RequestBody::default()
        .max_tokens(64)
        .system(json!([text_block(
            "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.\n"
        )]))
        .content(json!([text_block(user_text)]))
        .build()
}

async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    body: &Value,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", session_id)
        .body(serde_json::to_vec(body)?)
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "proxy returned {}", resp.status());
    let _ = resp.bytes().await?;
    Ok(())
}

async fn insert_agent(pool: &PgPool, name: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.agents (name, source_id) VALUES ($1, 1) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn insert_alias(pool: &PgPool, agent_id: i64, alias: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE agents.agents
            SET persona_aliases = ARRAY_APPEND(persona_aliases, $2)
          WHERE id = $1",
    )
    .bind(agent_id)
    .bind(alias)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_agent_lesson(
    pool: &PgPool,
    project_id: i64,
    agent_id: i64,
    kind: &str,
    text: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, agent_id, kind_id, text, stage_id, source_id)
         SELECT $1, $2, lk.id, $4, ls.id, lsrc.id
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = $3 AND ls.code = 'daily' AND lsrc.code = 'operator'",
    )
    .bind(project_id)
    .bind(agent_id)
    .bind(kind)
    .bind(text)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_project(pool: &PgPool, project_path: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

fn last_appended_block_text(body: &Value) -> Option<&str> {
    body["messages"]
        .as_array()
        .and_then(|msgs| msgs.last())
        .and_then(|m| m["content"].as_array())
        .and_then(|content| content.last())
        .and_then(|b| b["text"].as_str())
}

async fn assert_first_subagent_request_gets_injected(
    client: &reqwest::Client,
    proxy_url: &str,
    mock: &MockServer,
    session_a: &str,
    persona: &str,
    lesson_text: &str,
) -> anyhow::Result<()> {
    send_request(
        client,
        proxy_url,
        session_a,
        &main_session_body("hello from the main session"),
    )
    .await?;
    let first_body = subagent_body(persona, "please review this diff");
    send_request(client, proxy_url, session_a, &first_body).await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 2, "expected exactly 2 forwarded requests");

    let forwarded_first: Value = serde_json::from_slice(&received[1].body)?;
    assert_eq!(
        forwarded_first["system"], first_body["system"],
        "system array must forward byte-identical — the actuator splices only the messages tail"
    );
    let appended = last_appended_block_text(&forwarded_first).ok_or_else(|| {
        anyhow::anyhow!("forwarded body missing appended content block: {forwarded_first}")
    })?;
    assert!(
        appended.contains(lesson_text),
        "the seeded agent's lesson must appear in the forwarded body, got: {appended}"
    );
    assert!(
        appended.contains("<system-reminder source=\"proxy-agent-lessons\">"),
        "forwarded body must carry the proxy-agent-lessons reminder block, got: {appended}"
    );
    Ok(())
}

async fn assert_repeat_request_not_reinjected(
    client: &reqwest::Client,
    proxy_url: &str,
    mock: &MockServer,
    session_a: &str,
    persona: &str,
) -> anyhow::Result<()> {
    let second_body = subagent_body(persona, "please review this diff again");
    send_request(client, proxy_url, session_a, &second_body).await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 3, "expected exactly 3 forwarded requests");
    let forwarded_second: Value = serde_json::from_slice(&received[2].body)?;
    let second_messages = forwarded_second["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("second forwarded body missing messages"))?;
    let second_content = second_messages
        .last()
        .and_then(|m| m["content"].as_array())
        .ok_or_else(|| anyhow::anyhow!("second forwarded last-turn content missing"))?;
    assert_eq!(
        second_content.len(),
        1,
        "dedup: repeat request in the same session must NOT receive a second injection"
    );
    Ok(())
}

async fn assert_toggle_off_disables_injection(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
    mock: &MockServer,
    persona: &str,
) -> anyhow::Result<()> {
    let patch_resp = client
        .patch(format!("{inspect_url}/api/settings/features/agent_lessons"))
        .json(&json!({"enabled": false}))
        .send()
        .await?;
    anyhow::ensure!(
        patch_resp.status() == 200,
        "PATCH agent_lessons toggle returned {}",
        patch_resp.status()
    );

    let session_b = uuid::Uuid::new_v4().to_string();
    send_request(
        client,
        proxy_url,
        &session_b,
        &main_session_body("hello from the second main session"),
    )
    .await?;
    let third_body = subagent_body(persona, "one more review, toggle is off");
    send_request(client, proxy_url, &session_b, &third_body).await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 5, "expected exactly 5 forwarded requests");
    let forwarded_third: Value = serde_json::from_slice(&received[4].body)?;
    assert_eq!(
        forwarded_third, third_body,
        "with agent_lessons disabled, the forwarded body must be byte-identical to what was sent"
    );
    Ok(())
}

#[tokio::test]
async fn agent_lesson_injected_into_subagent_request() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (state, proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let persona = "Code Reviewer";
    let lesson_text = "ALWAYS run `make test`, never raw `cargo nextest` in this repo";

    let project_id = insert_project(&pool, "agent-lesson-proj").await?;
    let agent_id = insert_agent(&pool, "code-reviewer-agent").await?;
    insert_alias(&pool, agent_id, persona).await?;
    insert_agent_lesson(&pool, project_id, agent_id, "correction", lesson_text).await?;
    mike::lesson_worker::refresh_lesson_cache(&state).await;

    let arm_resp = client
        .patch(format!("{inspect_url}/api/settings/features/agent_lessons"))
        .json(&json!({"enabled": true}))
        .send()
        .await?;
    anyhow::ensure!(
        arm_resp.status() == 200,
        "PATCH agent_lessons toggle returned {}",
        arm_resp.status()
    );

    let session_a = uuid::Uuid::new_v4().to_string();
    assert_first_subagent_request_gets_injected(
        &client,
        &proxy_url,
        &mock,
        &session_a,
        persona,
        lesson_text,
    )
    .await?;
    assert_repeat_request_not_reinjected(&client, &proxy_url, &mock, &session_a, persona).await?;
    assert_toggle_off_disables_injection(&client, &proxy_url, &inspect_url, &mock, persona).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
