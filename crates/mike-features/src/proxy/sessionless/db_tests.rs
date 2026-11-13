use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use sqlx::Row;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::config::ProxyConfig;
use crate::proxy::engine::{AcceptOutcome, accept};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

struct Env {
    db: TestDbHandle,
    state: AppState,
    persist: PersistHandle,
}

async fn boot(upstream: &str) -> anyhow::Result<Option<Env>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &db.database_url,
    ])?;
    let state = AppState::new(config).await?;
    let persist = state
        .persist()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("persist_enabled config must yield a PersistHandle"))?;
    Ok(Some(Env { db, state, persist }))
}

struct Shape {
    method: &'static str,
    path: &'static str,
    user_agent: &'static str,
    session_header: Option<&'static str>,
    body: &'static [u8],
    content_type: &'static str,
    reply: &'static [u8],
    reason: &'static str,
}

const SHAPES: &[Shape] = &[
    Shape {
        method: "GET",
        path: "/v1/mcp_servers",
        user_agent: "claude-code/2.1.322",
        session_header: None,
        body: b"",
        content_type: "application/json",
        reply: br#"{"data":[]}"#,
        reason: "missing_session_header",
    },
    Shape {
        method: "POST",
        path: "/v1/design/mcp",
        user_agent: "claude-code/2.1.322 (cli)",
        session_header: None,
        body: br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        content_type: "text/event-stream",
        reply: b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n",
        reason: "missing_session_header",
    },
    Shape {
        method: "POST",
        path: "/v1/code/sessions",
        user_agent: "claude-code/2.1.322",
        session_header: None,
        body: br#"{"title":"t"}"#,
        content_type: "application/json",
        reply: br#"{"id":"cse_abc123"}"#,
        reason: "missing_session_header",
    },
    Shape {
        method: "GET",
        path: "/v1/ultrareview/quota",
        user_agent: "claude-cli/2.1.322 (external, cli)",
        session_header: None,
        body: b"",
        content_type: "application/json",
        reply: br#"{"remaining":3}"#,
        reason: "missing_session_header",
    },
    Shape {
        method: "POST",
        path: "/v1/messages/count_tokens",
        user_agent: "claude-cli/2.1.322",
        session_header: Some("not-a-uuid"),
        body: br#"{"model":"m","messages":[]}"#,
        content_type: "application/json",
        reply: br#"{"input_tokens":1}"#,
        reason: "malformed_session_header",
    },
];

async fn mount(upstream: &MockServer, shape: &Shape) {
    Mock::given(method(shape.method))
        .and(path(shape.path))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", shape.content_type)
                .set_body_bytes(shape.reply),
        )
        .expect(1)
        .mount(upstream)
        .await;
}

async fn send(state: &AppState, shape: &Shape) -> anyhow::Result<Vec<u8>> {
    let mut builder = Request::builder()
        .method(shape.method)
        .uri(shape.path)
        .header("user-agent", shape.user_agent)
        .header("authorization", "Bearer secret-token");
    if let Some(value) = shape.session_header {
        builder = builder.header("x-claude-code-session-id", value);
    }
    let request = builder.body(Body::from(shape.body))?;
    let AcceptOutcome::EarlyResponse(response) = accept(state, request).await? else {
        anyhow::bail!(
            "{} {} was accepted as a session request",
            shape.method,
            shape.path
        );
    };
    Ok(response.into_body().collect().await?.to_bytes().to_vec())
}

async fn stored_row(persist: &PersistHandle, path: &str) -> anyhow::Result<sqlx::postgres::PgRow> {
    for _ in 0..100 {
        let row = sqlx::query(
            "SELECT reason, method, user_agent, request_headers::text AS headers, request_body,
                    response_status, response_body, response_body_size, response_body_refused, status
               FROM requests.sessionless_requests WHERE path = $1",
        )
        .bind(path)
        .fetch_optional(persist.pool())
        .await?;
        if let Some(row) = row {
            return Ok(row);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("no sessionless row was recorded for {path}")
}

async fn assert_captured(env: &Env, shape: &Shape) -> anyhow::Result<()> {
    let client_saw = send(&env.state, shape).await?;
    assert_eq!(
        client_saw, shape.reply,
        "{}: the client gets upstream's bytes",
        shape.path
    );
    let row = stored_row(&env.persist, shape.path).await?;
    assert_eq!(
        row.try_get::<String, _>("reason")?,
        shape.reason,
        "{}",
        shape.path
    );
    assert_eq!(row.try_get::<String, _>("method")?, shape.method);
    assert_eq!(
        row.try_get::<Option<String>, _>("user_agent")?.as_deref(),
        Some(shape.user_agent)
    );
    assert_eq!(row.try_get::<Vec<u8>, _>("request_body")?, shape.body);
    assert_eq!(row.try_get::<Option<i16>, _>("response_status")?, Some(200));
    assert_eq!(
        row.try_get::<Option<Vec<u8>>, _>("response_body")?
            .as_deref(),
        Some(shape.reply)
    );
    assert_eq!(
        row.try_get::<Option<i64>, _>("response_body_size")?,
        Some(i64::try_from(shape.reply.len())?)
    );
    assert!(!row.try_get::<bool, _>("response_body_refused")?);
    assert_eq!(row.try_get::<String, _>("status")?, "complete");
    let headers = row.try_get::<String, _>("headers")?;
    assert!(
        !headers.contains("secret-token"),
        "{}: credentials are masked: {headers}",
        shape.path
    );
    Ok(())
}

#[tokio::test]
async fn sessionless_request_each_observed_shape_is_forwarded_and_stored_whole()
-> anyhow::Result<()> {
    let upstream = MockServer::start().await;
    for shape in SHAPES {
        mount(&upstream, shape).await;
    }
    let Some(env) = boot(&upstream.uri()).await? else {
        return Ok(());
    };
    for shape in SHAPES {
        assert_captured(&env, shape).await?;
    }
    env.state.shutdown().cancel();
    env.db.cleanup().await
}
