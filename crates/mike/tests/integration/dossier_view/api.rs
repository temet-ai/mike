use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::Value;
use tokio::net::TcpListener;

use super::seeded;
use crate::turn_facts::{Fixture, fixture};

async fn serve(f: &Fixture) -> anyhow::Result<(String, AppState)> {
    let args = vec![
        "mike".to_string(),
        "--inspect-listen-addr".to_string(),
        "127.0.0.1:0".to_string(),
        "--persist-enabled".to_string(),
        "1".to_string(),
        "--database-url".to_string(),
        f.db.database_url.clone(),
    ];
    let state = AppState::new(ProxyConfig::try_parse_from(args)?).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let router = mike::app::inspect_router_on(state.clone(), listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Ok((base, state))
}

async fn session_uuid(f: &Fixture) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT session_uuid::text FROM sessions.sessions WHERE id = $1")
            .bind(f.session_id)
            .fetch_one(f.handle.pool())
            .await?,
    )
}

async fn get(url: String) -> anyhow::Result<Value> {
    let response = reqwest::get(&url).await?;
    let status = response.status();
    let body: Value = response.json().await?;
    anyhow::ensure!(status.is_success(), "GET {url}: {status}: {body}");
    Ok(body)
}

#[tokio::test]
async fn the_dossier_route_returns_whole_items_and_pages() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (_, _, third) = seeded(&f).await?;
    let (base, state) = serve(&f).await?;
    let uuid = session_uuid(&f).await?;
    let body = get(format!(
        "{base}/api/sessions/{uuid}/dossier?limit=2&audience=steerer&turn_id={third}"
    ))
    .await?;
    assert_eq!(body["renderer_version"], "dossier-r1");
    assert_eq!(body["view"]["stream"]["total"], 3);
    assert_eq!(
        body["view"]["stream"]["items"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(body["view"]["stream"]["next_offset"], 2);
    assert_eq!(
        body["view"]["stream"]["items"][0]["text"],
        "Refactor the store. Then commit when green."
    );
    assert!(body["rendered"]["text"].is_string());
    let next = get(format!(
        "{base}/api/sessions/{uuid}/dossier?limit=2&offset=2&section=stream"
    ))
    .await?;
    assert_eq!(next["view"]["stream"]["items"][0]["text"], "thanks");
    assert!(next["view"].get("turns").is_none());
    let facts = get(format!("{base}/api/operator-turns/{third}/facts")).await?;
    assert_eq!(facts["turn_id"], third);
    let stream = get(format!("{base}/api/sessions/{uuid}/prompt-stream")).await?;
    assert_eq!(stream["stream"]["total"], 3);
    state.shutdown().cancel();
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_judge_audience_on_an_open_turn_is_422() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let open = crate::turn_facts::resolution::message(&f, 9100, 0, "still going", None).await?;
    let (base, state) = serve(&f).await?;
    let uuid = session_uuid(&f).await?;
    let response = reqwest::get(format!(
        "{base}/api/sessions/{uuid}/dossier?audience=judge&turn_id={open}"
    ))
    .await?;
    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    state.shutdown().cancel();
    f.db.cleanup().await?;
    Ok(())
}
