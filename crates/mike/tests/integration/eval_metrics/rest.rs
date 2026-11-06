use mike::config::{AppState, ProxyConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

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

#[tokio::test]
async fn list_eval_alarms_then_acknowledge_round_trip() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let detail = json!({"waiting": 12, "calls": {"ok": 3}});
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO eval.alarms (kind, subject, detail) VALUES ('judge_stalled', 'turn_judge', $1) RETURNING id",
    )
    .bind(&detail)
    .fetch_one(f.handle.pool())
    .await?;
    let (base, state) = serve(&f).await?;
    let listed: Value = reqwest::get(format!("{base}/api/eval/alarms"))
        .await?
        .json()
        .await?;
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["alarms"][0]["detail"], detail);
    let client = reqwest::Client::new();
    let url = format!("{base}/api/eval/alarms/{id}/acknowledge");
    let acked = client
        .patch(&url)
        .json(&json!({"note": "known stall"}))
        .send()
        .await?;
    assert_eq!(acked.status().as_u16(), 200);
    let body: Value = acked.json().await?;
    assert_eq!(body["ack_note"], "known stall");
    assert!(body["cleared_at"].is_null());
    let again = client
        .patch(&url)
        .json(&json!({"note": "again"}))
        .send()
        .await?;
    assert_eq!(again.status().as_u16(), 409);
    let missing = client
        .patch(format!("{base}/api/eval/alarms/987654/acknowledge"))
        .json(&json!({"note": "x"}))
        .send()
        .await?;
    assert_eq!(missing.status().as_u16(), 404);
    state.shutdown().cancel();
    Ok(())
}

#[tokio::test]
async fn get_eval_rates_limit_above_max_returns_400() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (base, state) = serve(&f).await?;
    let resp = reqwest::get(format!("{base}/api/eval/rates?limit=1001")).await?;
    assert_eq!(resp.status().as_u16(), 400);
    let ok: Value = reqwest::get(format!("{base}/api/eval/rates"))
        .await?
        .json()
        .await?;
    assert_eq!(ok["total"], 0);
    assert!(ok["rollup"].is_null());
    state.shutdown().cancel();
    Ok(())
}

#[test]
fn acknowledge_route_is_exempt_from_mcp_in_manifest() {
    let manifest = mike::api::manifest::inspect_route_manifest();
    let ack = manifest
        .iter()
        .find(|r| r.path == "/api/eval/alarms/{id}/acknowledge");
    assert!(
        ack.is_some_and(|r| matches!(r.coverage, mike::api::manifest::Coverage::Exempt { .. }))
    );
}
