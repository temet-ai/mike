use mike::config::ProxyConfig;
use mike::state::AppState;
use mike::store::persistence::steering_evaluations::{EvaluationMode, NewEvaluation};
use serde_json::Value;
use tokio::net::TcpListener;

use crate::store::operator_turns::support::seed_open_turn_fixture;

async fn serve(database_url: &str) -> anyhow::Result<(String, AppState)> {
    let args = vec![
        "mike".to_string(),
        "--inspect-listen-addr".to_string(),
        "127.0.0.1:0".to_string(),
        "--persist-enabled".to_string(),
        "1".to_string(),
        "--database-url".to_string(),
        database_url.to_owned(),
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
async fn the_evaluations_route_pages_to_the_end_and_stats_match_the_list() -> anyhow::Result<()> {
    let Some(f) = seed_open_turn_fixture().await? else {
        return Ok(());
    };
    let steerer_id: i64 =
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE kind = 'verifier' LIMIT 1")
            .fetch_one(&f.pool)
            .await?;
    let session_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&f.session)
            .fetch_one(&f.pool)
            .await?;
    for i in 0..5 {
        let new = NewEvaluation {
            steerer_id,
            session_id,
            operator_turn_id: f.turn_id,
            mode: if i % 2 == 0 {
                EvaluationMode::Live
            } else {
                EvaluationMode::Shadow
            },
            bundle_id: None,
            trigger_point: Some("turn_end"),
            trigger_request_id: Some(f.request_id),
        };
        let uuid = f.handle.insert_evaluation_dispatched(&new).await?;
        f.handle
            .resolve_evaluation(uuid, i == 1, None, None)
            .await?;
    }
    let (base, state) = serve(&f.db.database_url).await?;
    let mut seen = Vec::new();
    let mut cursor: Option<i64> = None;
    loop {
        let url = match cursor {
            Some(id) => format!(
                "{base}/api/sessions/{}/evaluations?limit=2&before_id={id}",
                f.session
            ),
            None => format!("{base}/api/sessions/{}/evaluations?limit=2", f.session),
        };
        let page: Value = reqwest::get(url).await?.json().await?;
        assert_eq!(page["total_count"], 5);
        for row in page["evaluations"].as_array().into_iter().flatten() {
            seen.push(row["id"].as_i64().unwrap_or_default());
        }
        cursor = page["next_before_id"].as_i64();
        if cursor.is_none() {
            break;
        }
    }
    let mut unique = seen.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(seen.len(), 5);
    assert_eq!(unique.len(), 5);

    let stats: Value = reqwest::get(format!(
        "{base}/api/steering/evaluation-stats?steerer_id={steerer_id}"
    ))
    .await?
    .json()
    .await?;
    let total: i64 = stats["stats"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["evaluations"].as_i64())
        .sum();
    assert_eq!(total, 5);
    state.shutdown().cancel();
    f.pool.close().await;
    f.db.cleanup().await?;
    Ok(())
}
