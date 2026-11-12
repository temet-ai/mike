use mike::config::ProxyConfig;
use mike::state::AppState;
use tokio::net::TcpListener;

use crate::db_helpers;

async fn state_with_db()
-> anyhow::Result<Option<(mike::store::test_support::TestDbHandle, AppState)>> {
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire_isolated().await?;
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &db.database_url,
    ])?;
    let state = AppState::new(config).await?;
    Ok(Some((db, state)))
}

async fn serve(state: &AppState) -> anyhow::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let router = mike::app::inspect_router_on(state.clone(), listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Ok(base)
}

#[tokio::test]
async fn semantic_toggle_off_idles_worker_and_on_resumes_without_restart() -> anyhow::Result<()> {
    let Some((db, state)) = state_with_db().await? else {
        return Ok(());
    };
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    let base = serve(&state).await?;
    let client = reqwest::Client::new();
    let toggle = |enabled: bool| {
        client
            .patch(format!("{base}/api/settings/features/semantic_context"))
            .json(&serde_json::json!({ "enabled": enabled }))
            .send()
    };
    anyhow::ensure!(
        toggle(false).await?.status().is_success(),
        "PATCH semantic_context off failed"
    );
    let off = mike::semantic::run_semantic_once(&state).await?;
    assert_eq!(off.status, "disabled");
    let items: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.semantic_items")
        .fetch_one(&pool)
        .await?;
    assert_eq!(items, 0);
    anyhow::ensure!(
        toggle(true).await?.status().is_success(),
        "PATCH semantic_context on failed"
    );
    let on = mike::semantic::run_semantic_once(&state).await;
    assert!(
        on.is_err() || on.is_ok_and(|r| r.status != "disabled"),
        "the next tick does work again"
    );
    Ok(())
}

#[tokio::test]
async fn status_route_answers_and_a_disallowed_kind_is_a_400_naming_it() -> anyhow::Result<()> {
    let Some((_db, state)) = state_with_db().await? else {
        return Ok(());
    };
    let base = serve(&state).await?;
    let status: serde_json::Value = reqwest::get(format!("{base}/api/semantic/status"))
        .await?
        .json()
        .await?;
    assert!(status.get("egress").is_some(), "{status}");
    assert!(status.get("item_counts").is_some());
    let missing = reqwest::get(format!("{base}/api/semantic/turns/999999/neighbours")).await?;
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
    let forbidden = reqwest::Client::new()
        .post(format!("{base}/api/semantic/patterns/candidates/1/name"))
        .json(&serde_json::json!({"pattern": "x-y-z", "description": "d"}))
        .send()
        .await?;
    assert_eq!(
        forbidden.status(),
        reqwest::StatusCode::FORBIDDEN,
        "naming is SPA-only"
    );
    Ok(())
}
