use crate::db_helpers;

use chrono::Utc;
use mike::store::persistence::turn_verdicts::{
    TurnVerdictInsert, VerdictCallUsage, VerdictJudgement, VerdictMode, VerdictShape,
};
use tokio::net::TcpListener;
use wiremock::MockServer;

use super::support::{seed_unjudged_turns, start_proxy_with_judge};

fn verdict<'a>(
    turn_id: i64,
    mode: VerdictMode,
    bundle_id: Option<i64>,
    input_text: &'a str,
) -> TurnVerdictInsert<'a> {
    TurnVerdictInsert {
        turn_id,
        mode,
        bundle_id,
        rubric_version: "v2-2026-09-23",
        input_builder_version: "judge-input-v2.1",
        slices: Vec::new(),
        model: "google/gemini-3.5-flash",
        judgement: VerdictJudgement {
            shape: VerdictShape::TwoAxis {
                reaction: "accepted",
                delivery: "delivered",
                failure_tags: None,
                severity: None,
            },
            confidence: 0.8,
            rationale: "the operator moved on",
            unmet_ask: "",
            evidence_quality: "asserted",
        },
        next_genuine_request_id: None,
        as_of: Utc::now(),
        input_text,
        usage: VerdictCallUsage::default(),
    }
}

async fn serve_inspect(state: &mike::state::AppState) -> anyhow::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let router = mike::app::inspect_router_on(state.clone(), listener.local_addr()?);
    tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });
    Ok(base)
}

#[tokio::test]
async fn get_turn_verdicts_returns_live_shadow_and_eval_rows_whole_and_filters_by_mode()
-> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;
    let Some(factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let session_id = "12345678-f00a-4f0a-8f0a-f0af0af0af01";
    seed_unjudged_turns(&pool, "test-verdicts-api", session_id, 1).await?;
    let turn_id: i64 = sqlx::query_scalar("SELECT max(id) FROM memory.operator_turns")
        .fetch_one(&pool)
        .await?;
    let bundle_id: i64 = sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha, build_dirty, cause)
         VALUES ('turn_judge', '', 'candidate', 'candidate', '{}'::jsonb, 'sha-test', 'build', false, '{}'::jsonb)
         RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    let (_proxy, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;
    let persist = state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence is off"))?;
    let long = "y".repeat(20_000);
    for mode in [VerdictMode::Live, VerdictMode::Shadow, VerdictMode::Eval] {
        persist
            .insert_turn_verdict(verdict(turn_id, mode, Some(bundle_id), &long))
            .await?;
    }
    let base = serve_inspect(&state).await?;

    let all: serde_json::Value =
        reqwest::get(format!("{base}/api/operator-turns/{turn_id}/verdicts"))
            .await?
            .json()
            .await?;
    assert_eq!(all["total"], 3);
    let modes: Vec<&str> = all["verdicts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["mode"].as_str())
        .collect();
    assert_eq!(modes, ["eval", "shadow", "live"]);
    assert_eq!(
        all["verdicts"][0]["input_text"].as_str().map(str::len),
        Some(20_000)
    );

    let shadow: serde_json::Value = reqwest::get(format!(
        "{base}/api/operator-turns/{turn_id}/verdicts?mode=shadow"
    ))
    .await?
    .json()
    .await?;
    assert_eq!(shadow["total"], 1);
    assert_eq!(shadow["verdicts"][0]["mode"], "shadow");

    let missing = reqwest::get(format!("{base}/api/operator-turns/987654321/verdicts")).await?;
    assert_eq!(missing.status().as_u16(), 404);

    let page: serde_json::Value =
        reqwest::get(format!("{base}/api/turn-verdicts?limit=2&offset=0"))
            .await?
            .json()
            .await?;
    assert_eq!(page["total"], 3);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(2));

    let bad = reqwest::get(format!("{base}/api/turn-verdicts?mode=projected")).await?;
    assert_eq!(bad.status().as_u16(), 400);

    shutdown.cancel();
    Ok(())
}
