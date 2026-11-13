use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;

use crate::test_db::TestDb;

use mike::api::manifest::inspect_router;
use mike::config::ProxyConfig;
use mike::state::AppState;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::workspace::RegisterWorkspaceSession;

struct Env {
    db: mike::store::test_support::TestDbHandle,
    persist: PersistHandle,
    router: Router,
    shutdown: CancellationToken,
}

impl Env {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(factory) = TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let db = factory.acquire_isolated().await?;
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            "https://api.anthropic.com",
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:9989",
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
        let shutdown = state.shutdown();
        let router = inspect_router(state);
        Ok(Some(Self {
            db,
            persist,
            router,
            shutdown,
        }))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.shutdown.cancel();
        self.db.cleanup().await
    }
}

fn encode_query_value(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

async fn get_flow(
    router: &Router,
    session: Uuid,
    query: &str,
) -> anyhow::Result<(StatusCode, Value)> {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .header("host", "127.0.0.1:9989")
                .uri(format!("/api/sessions/{session}/flow?{query}"))
                .body(Body::empty())?,
        )
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    Ok((status, body))
}

async fn register_eligible_session(persist: &PersistHandle) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    mike::store::persistence::resolve_session_id(
        persist.pool(),
        &session.to_string(),
        None,
        Some(1),
    )
    .await?;
    persist
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: None,
            cwd: "/tmp/flow-api-eligible",
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    Ok(session)
}

async fn register_unadmitted_session(persist: &PersistHandle) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    persist
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: None,
            cwd: "/tmp/flow-api-unadmitted",
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    Ok(session)
}

async fn seed_request(persist: &PersistHandle, session: Uuid) -> anyhow::Result<i64> {
    let (session_db_id, project_id): (i64, i64) =
        sqlx::query_as("SELECT id, project_id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(persist.pool())
            .await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO requests.requests
         (session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
          status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
          created_at, persisted_at, request_body)
         SELECT $1, $2, m.id, '/v1/messages', true, 1, false, s.id,
                '{}'::jsonb, '', '', t.id, o.id, NOW(), NOW(), '\\x00'::bytea
         FROM requests.methods m, requests.statuses s, requests.turn_types t, requests.turn_origins o
         WHERE m.code = 'POST' AND s.code = 'pending' AND t.code = 'user_prompt' AND o.code = 'primary'
         RETURNING id",
    )
    .bind(session_db_id)
    .bind(project_id)
    .fetch_one(persist.pool())
    .await?)
}

#[tokio::test]
async fn unknown_session_returns_404() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let (status, _) = get_flow(&env.router, Uuid::new_v4(), "component=requests").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    env.cleanup().await
}

#[tokio::test]
async fn malformed_session_id_returns_404_not_400() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let response = env
        .router
        .clone()
        .oneshot(
            Request::builder()
                .header("host", "127.0.0.1:9989")
                .uri("/api/sessions/not-a-uuid/flow?component=requests")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    env.cleanup().await
}

#[tokio::test]
async fn unadmitted_session_returns_typed_unavailable_not_an_error() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let session = register_unadmitted_session(&env.persist).await?;
    let (status, body) = get_flow(&env.router, session, "component=requests").await?;
    assert_eq!(
        status,
        StatusCode::OK,
        "an unadmitted session is a 200, not an error"
    );
    assert_eq!(
        body["availability"],
        Value::String("no_admitted_start".into())
    );
    assert_eq!(body["activities"], Value::Array(vec![]));
    env.cleanup().await
}

#[tokio::test]
async fn unsupported_component_value_returns_400() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let session = register_eligible_session(&env.persist).await?;
    let (status, _) = get_flow(&env.router, session, "component=not_a_real_component").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    env.cleanup().await
}

#[tokio::test]
async fn every_wire_component_round_trips_through_persistence() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let session = register_eligible_session(&env.persist).await?;
    for component in [
        "requests",
        "dispatches",
        "tools",
        "results",
        "messages",
        "worker_status",
        "manifest_stages",
        "lesson_injections",
        "verifier_evaluations",
        "judgments",
        "llm_calls",
        "embeddings",
        "consolidation",
        "interventions",
    ] {
        let (status, body) =
            get_flow(&env.router, session, &format!("component={component}")).await?;
        assert_eq!(
            status,
            StatusCode::OK,
            "component {component} must return 200"
        );
        assert_eq!(
            body["coverage"]["component"],
            Value::String(component.into()),
            "coverage.component must echo the requested component for {component}"
        );
    }
    env.cleanup().await
}

#[tokio::test]
async fn cursor_continuation_advances_through_pages_to_exhausted() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let session = register_eligible_session(&env.persist).await?;
    let first_id = seed_request(&env.persist, session).await?;
    let second_id = seed_request(&env.persist, session).await?;

    let (status, first_page) = get_flow(&env.router, session, "component=requests&limit=1").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first_page["coverage"]["exhausted"], Value::Bool(false));
    let first_activity_request_id = first_page["activities"][0]["source"]["request_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("expected exactly one activity on the first page"))?;
    assert_eq!(first_activity_request_id, first_id.to_string());
    let cursor = first_page["next_cursor"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("a non-exhausted page must carry a next_cursor"))?;

    let (status, second_page) = get_flow(
        &env.router,
        session,
        &format!(
            "component=requests&limit=1&cursor={}",
            encode_query_value(cursor)
        ),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second_page["coverage"]["exhausted"], Value::Bool(true));
    assert_eq!(second_page["next_cursor"], Value::Null);
    let second_activity_request_id = second_page["activities"][0]["source"]["request_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("expected exactly one activity on the second page"))?;
    assert_eq!(second_activity_request_id, second_id.to_string());

    env.cleanup().await
}

#[tokio::test]
async fn malformed_cursor_returns_400() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let session = register_eligible_session(&env.persist).await?;

    let (status, _) = get_flow(&env.router, session, "component=requests&cursor=not-json").await?;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "cursor that isn't JSON must 400"
    );

    let foreign_cursor = serde_json::json!({
        "version": 1,
        "session_id": session.to_string(),
        "component": "tools",
        "filter": "all",
        "after_id": "1",
        "upper_id": "2",
    })
    .to_string();
    let (status, _) = get_flow(
        &env.router,
        session,
        &format!(
            "component=requests&cursor={}",
            encode_query_value(&foreign_cursor)
        ),
    )
    .await?;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a cursor scoped to another component must 400"
    );

    env.cleanup().await
}

#[tokio::test]
async fn eligible_session_export_route_returns_coverage_per_component() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        "https://api.anthropic.com",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:9989",
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
    let shutdown = state.shutdown();
    let router: Router = inspect_router(state);

    let session = Uuid::new_v4();
    mike::store::persistence::resolve_session_id(
        persist.pool(),
        &session.to_string(),
        None,
        Some(1),
    )
    .await?;
    persist
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: None,
            cwd: "/tmp/flow-export-eligible",
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;

    let response = router
        .oneshot(
            Request::builder()
                .header("host", "127.0.0.1:9989")
                .uri(format!("/api/sessions/{session}/flow/export"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["otherData"]["session_id"], session.to_string());
    let coverage = body["otherData"]["coverage"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("coverage must be an array"))?;
    assert_eq!(
        coverage.len(),
        14,
        "one merged coverage entry per exported component"
    );
    assert!(body["traceEvents"].as_array().is_some());

    shutdown.cancel();
    db.cleanup().await
}
