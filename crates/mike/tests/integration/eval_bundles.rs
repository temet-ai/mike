use mike::config::{AppState, ProxyConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::test_db::{TestDb, TestDbHandle};

struct Stack {
    base: String,
    state: AppState,
    db: TestDbHandle,
}

async fn state_for(url: &str) -> anyhow::Result<AppState> {
    let args = vec![
        "mike".to_string(),
        "--inspect-listen-addr".to_string(),
        "127.0.0.1:0".to_string(),
        "--persist-enabled".to_string(),
        "1".to_string(),
        "--database-url".to_string(),
        url.to_string(),
    ];
    AppState::new(ProxyConfig::try_parse_from(args)?).await
}

impl Stack {
    async fn start() -> anyhow::Result<Option<Self>> {
        let Some(factory) = TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let db = factory.acquire_isolated().await?;
        let state = state_for(&db.database_url).await?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}", listener.local_addr()?);
        let router = mike::app::inspect_router_on(state.clone(), listener.local_addr()?);
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Some(Self { base, state, db }))
    }

    async fn get(&self, route: &str) -> anyhow::Result<Value> {
        let response = reqwest::Client::new()
            .get(format!("{}{route}", self.base))
            .send()
            .await?;
        let status = response.status();
        let value: Value = response.json().await?;
        anyhow::ensure!(status.is_success(), "GET {route}: {status}: {value}");
        Ok(value)
    }

    async fn patch(&self, route: &str, body: Value) -> anyhow::Result<()> {
        let response = reqwest::Client::new()
            .patch(format!("{}{route}", self.base))
            .json(&body)
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "PATCH {route}: {}",
            response.status()
        );
        Ok(())
    }

    async fn bundles(&self, query: &str) -> anyhow::Result<Vec<Value>> {
        let page = self
            .get(&format!("/api/eval/bundles?limit=200&{query}"))
            .await?;
        Ok(page["bundles"].as_array().cloned().unwrap_or_default())
    }

    async fn finish(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await?;
        Ok(())
    }
}

async fn live_steerers(state: &AppState) -> anyhow::Result<i64> {
    let persist = state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on"))?;
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM steering.steerers WHERE deleted_at IS NULL")
            .fetch_one(persist.pool())
            .await?,
    )
}

#[tokio::test]
async fn a_fresh_db_gets_one_current_bundle_per_steerer_plus_the_two_judges() -> anyhow::Result<()>
{
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let steerers = live_steerers(&stack.state).await?;
    let current = stack.bundles("current=true").await?;
    assert_eq!(current.len() as i64, steerers + 2, "current: {current:?}");
    let turn_judge = current
        .iter()
        .find(|b| b["role"] == json!("turn_judge"))
        .ok_or_else(|| anyhow::anyhow!("no turn_judge bundle"))?;
    assert!(
        turn_judge["components"]["system_prompt"]
            .as_str()
            .is_some_and(|p| p.starts_with("You are a turn-outcome judge.")),
        "the compiled prompt is stored whole"
    );
    let ids = stack.state.settings_overlay.load().bundles.clone();
    assert_eq!(ids.turn_judge, turn_judge["id"].as_i64());
    stack.finish().await
}

#[tokio::test]
async fn a_model_change_and_back_makes_two_bundles_naming_their_change_rows() -> anyhow::Result<()>
{
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let route = "/api/settings/judge_worker.model";
    stack
        .patch(route, json!({"value": "openai/gpt-5-mini"}))
        .await?;
    stack.patch(route, json!({"value": ""})).await?;
    let rows = stack.bundles("role=turn_judge").await?;
    assert_eq!(rows.len(), 3, "startup plus two changes: {rows:?}");
    let changes = stack
        .get("/api/config/change-log?subject_kind=setting&subject_key=judge_worker.model&limit=10")
        .await?;
    let change_ids: Vec<i64> = changes["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["op"] != json!("observe"))
        .filter_map(|row| row["id"].as_i64())
        .collect();
    for (bundle, change_id) in rows.iter().take(2).zip(change_ids.iter()) {
        assert_eq!(bundle["cause"]["trigger"], "setting");
        assert_eq!(bundle["cause"]["change_log_ids"], json!([change_id]));
    }
    stack.finish().await
}

#[tokio::test]
async fn a_restart_with_no_change_creates_no_bundle() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let before = stack.bundles("").await?.len();
    let again = state_for(&stack.db.database_url).await?;
    again.shutdown().cancel();
    assert_eq!(stack.bundles("").await?.len(), before);
    stack.finish().await
}

#[tokio::test]
async fn a_steerer_prompt_patch_supersedes_only_that_steerers_bundle() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let current = stack.bundles("current=true&role=steerer").await?;
    let Some(first) = current.first() else {
        return stack.finish().await;
    };
    let steerer_id = first["subject_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("steerer bundle without subject_key"))?
        .to_owned();
    stack
        .patch(
            &format!("/api/steerers/{steerer_id}"),
            json!({"prompt": "judge only the final message"}),
        )
        .await?;
    let after = stack.bundles("current=true&role=steerer").await?;
    let changed: Vec<&Value> = after
        .iter()
        .filter(|b| !current.iter().any(|c| c["id"] == b["id"]))
        .collect();
    assert_eq!(changed.len(), 1, "after: {after:?}");
    assert_eq!(changed[0]["subject_key"], json!(steerer_id));
    assert_eq!(changed[0]["components"]["prompt_source"], "row");
    stack.finish().await
}

#[tokio::test]
async fn stamped_writers_carry_their_bundle_ids() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let persist = stack
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on"))?;
    let project_id =
        mike::store::test_support::seed_project(persist.pool(), "bundle-stamps").await?;
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, now(), now()) RETURNING id",
    )
    .bind(project_id)
    .fetch_one(persist.pool())
    .await?;
    let bundle = stack.state.settings_overlay.load().bundles.session_judge;
    let verdict_id = mike::store::persistence::judge::insert_session_verdict(
        persist.pool(),
        mike::store::persistence::judge::SessionVerdictInsert {
            session_db_id: session_id,
            outcome_id: 2,
            quality_score: Some(0.9),
            rationale: Some("fine"),
            judge_model: "openai/gpt-test",
            rubric_version: 2,
            judged_at: chrono::Utc::now(),
            bundle_id: bundle,
        },
    )
    .await?;
    let stored: Option<i64> =
        sqlx::query_scalar("SELECT bundle_id FROM session_judges.verdicts WHERE id = $1")
            .bind(verdict_id)
            .fetch_one(persist.pool())
            .await?;
    assert_eq!(stored, bundle);
    stack.finish().await
}
