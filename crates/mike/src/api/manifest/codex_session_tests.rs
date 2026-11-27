use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

use crate::config::ProxyConfig;
use crate::runtime::{ProviderThreadRef, RuntimeKind, RuntimeNamespace, RuntimeThreadBinding};
use crate::state::AppState;
use crate::store::persistence::pool_classes::PoolClasses;
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use crate::store::persistence::{PersistHandle, test_support};

async fn request(state: AppState, thread: &str) -> anyhow::Result<(StatusCode, Value)> {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("thread_id", thread)
        .finish();
    let response = super::register(Router::new())
        .with_state(state)
        .oneshot(
            Request::builder()
                .uri(format!("/api/codex/session?{query}"))
                .body(Body::empty())?,
        )
        .await?;
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), 16 * 1024).await?;
    Ok((status, serde_json::from_slice(&body)?))
}

async fn bind(persist: &PersistHandle, thread: &str) -> anyhow::Result<Uuid> {
    let namespace = RuntimeNamespace {
        id: Uuid::new_v4(),
        runtime: RuntimeKind::Codex,
        durable_key: format!("codex-session-route/{}", Uuid::new_v4()),
    };
    let session = Uuid::new_v4();
    persist.ensure_runtime_session(session).await?;
    persist
        .runtime()
        .register_runtime_namespace(&namespace)
        .await?;
    persist
        .runtime()
        .bind_runtime_thread(&RuntimeThreadBinding {
            mike_session_id: session,
            provider: ProviderThreadRef {
                namespace_id: namespace.id,
                thread_id: thread.to_owned(),
            },
            provider_session_id: None,
            owning_seat_id: None,
            native_parent: None,
            forked_from: None,
        })
        .await?;
    Ok(session)
}

#[tokio::test]
async fn codex_session_native_uuid_returns_canonical_session_and_ambiguity_refuses()
-> anyhow::Result<()> {
    let db = test_support::acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("Codex session route requires the test database"))?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(persist.clone());
    let thread = Uuid::new_v4().to_string();
    let session = bind(&persist, &thread).await?;
    assert_ne!(thread, session.to_string());
    let (status, body) = request(state.clone(), &thread).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["session_id"], session.to_string());
    bind(&persist, &thread).await?;
    let (status, body) = request(state.clone(), &thread).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "codex_session_unresolved");
    state.shutdown().cancel();
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn codex_session_opaque_thread_preserves_whitespace_and_unknown_refuses() -> anyhow::Result<()>
{
    let db = test_support::acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("Codex session route requires the test database"))?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(persist.clone());
    let thread = format!(" Thread:Case/{} +é ", Uuid::new_v4());
    let session = bind(&persist, &thread).await?;
    let (status, body) = request(state.clone(), &thread).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["session_id"], session.to_string());
    let (status, body) = request(state.clone(), thread.trim()).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "codex_session_unresolved");
    state.shutdown().cancel();
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn codex_session_invalid_thread_returns_bad_request() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    for thread in ["", " \t\n", "thread\0id", &"a".repeat(513)] {
        let (status, body) = request(state.clone(), thread).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid_thread_id");
    }
    state.shutdown().cancel();
    Ok(())
}

#[tokio::test]
async fn codex_session_without_persistence_returns_unavailable() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let (status, body) = request(state.clone(), "unbound-native-thread").await?;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "persistence_disabled");
    state.shutdown().cancel();
    Ok(())
}

#[tokio::test]
async fn codex_session_failed_lookup_returns_error_without_identity() -> anyhow::Result<()> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost/codex_session_closed")?;
    pool.close().await;
    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(PersistHandle::from_pools(PoolClasses::single(pool)));
    let (status, body) = request(state.clone(), "native-thread").await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "codex_session_lookup_failed");
    assert!(body.get("session_id").is_none());
    state.shutdown().cancel();
    Ok(())
}
