use crate::anthropic_fixtures::{MockResponse, RequestBody};
use crate::test_db;

use std::time::Duration;

use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MAIN_SYSTEM: &str = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.";
const SESSION: &str = "550e8400-e29b-41d4-a716-4466554400d1";

struct Stack {
    proxy_url: String,
    inspect_url: String,
    state: AppState,
    shutdown: CancellationToken,
    pool: PgPool,
    test_db: test_db::TestDbHandle,
    upstream: MockServer,
}

impl Stack {
    async fn start(backstop_secs: Option<i64>) -> anyhow::Result<Option<Self>> {
        let Some(factory) = test_db::TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let test_db = factory.acquire_isolated().await?;
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(MockResponse::new("msg_dossier").minimal().build()),
            )
            .mount(&upstream)
            .await;
        let args = vec![
            "mike".to_string(),
            "--upstream-url".to_string(),
            upstream.uri(),
            "--proxy-listen-addr".to_string(),
            "127.0.0.1:0".to_string(),
            "--inspect-listen-addr".to_string(),
            "127.0.0.1:0".to_string(),
            "--persist-enabled".to_string(),
            "1".to_string(),
            "--database-url".to_string(),
            test_db.database_url.clone(),
        ];
        let state = AppState::new(ProxyConfig::try_parse_from(args)?).await?;
        let shutdown = state.shutdown();
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
        let proxy_url = format!("http://{}", proxy_listener.local_addr()?);
        let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
        let inspect_url = format!("http://{}", inspect_listener.local_addr()?);
        let proxy_app = proxy_router(state.clone());
        let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);
        let proxy_shutdown = shutdown.clone();
        tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, proxy_app)
                .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
                .await;
        });
        let inspect_shutdown = shutdown.clone();
        tokio::spawn(async move {
            let _ = axum::serve(inspect_listener, inspect_app)
                .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
                .await;
        });
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&test_db.database_url)
            .await?;
        if let Some(seconds) = backstop_secs {
            sqlx::query(
                "INSERT INTO config.settings (key, value)
                 VALUES ('dossier.backstop_interval_secs', to_jsonb($1::bigint))",
            )
            .bind(seconds)
            .execute(&pool)
            .await?;
        }
        mike::dossier::worker::spawn_dossier_builder(&state);
        Ok(Some(Self {
            proxy_url,
            inspect_url,
            state,
            shutdown,
            pool,
            test_db,
            upstream,
        }))
    }

    async fn finish(self) -> anyhow::Result<()> {
        self.shutdown.cancel();
        self.pool.close().await;
        self.test_db.cleanup().await?;
        Ok(())
    }

    async fn post_prompt(&self, text: &str) -> anyhow::Result<()> {
        let body: Value = RequestBody::default()
            .system(json!([{"type": "text", "text": MAIN_SYSTEM}]))
            .content(text)
            .build();
        let response = reqwest::Client::new()
            .post(format!("{}/v1/messages", self.proxy_url))
            .header("x-api-key", "sk-test")
            .header("anthropic-version", "2023-06-01")
            .header("x-claude-code-session-id", SESSION)
            .json(&body)
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "proxy answered {}",
            response.status()
        );
        let _ = response.bytes().await?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while self.state.in_flight_len() != 0 {
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "in flight never drained"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        Ok(())
    }

    async fn set_toggle(&self, enabled: bool) -> anyhow::Result<()> {
        let response = reqwest::Client::new()
            .patch(format!(
                "{}/api/settings/features/dossier_builder",
                self.inspect_url
            ))
            .json(&json!({"enabled": enabled}))
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "toggle patch: {}",
            response.status()
        );
        Ok(())
    }

    async fn facts_count(&self) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar("SELECT count(*) FROM memory.turn_facts")
            .fetch_one(&self.pool)
            .await?)
    }

    async fn wait_for_facts(&self, at_least: i64, within: Duration) -> anyhow::Result<i64> {
        let deadline = std::time::Instant::now() + within;
        loop {
            let count = self.facts_count().await?;
            if count >= at_least || std::time::Instant::now() > deadline {
                return Ok(count);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct FactsProbe {
    prompt_kind: String,
    next_state: String,
    next_genuine_request_id: Option<i64>,
    session_class: String,
    lag_ms: i64,
}

#[tokio::test]
async fn genuine_reply_closes_turn_and_facts_row_appears_within_flush_interval()
-> anyhow::Result<()> {
    let Some(stack) = Stack::start(None).await? else {
        return Ok(());
    };
    stack.set_toggle(true).await?;
    stack.post_prompt("please refactor the store").await?;
    stack.post_prompt("now write the docs for it").await?;
    let count = stack.wait_for_facts(1, Duration::from_secs(20)).await?;
    assert_eq!(count, 1, "the closed turn must gain a facts row");
    let probe: FactsProbe = sqlx::query_as(
        "SELECT f.prompt_kind, f.next_state, f.next_genuine_request_id, f.session_class,
                (EXTRACT(EPOCH FROM (f.built_at - t.closed_at)) * 1000)::bigint AS lag_ms
           FROM memory.turn_facts f
           JOIN memory.operator_turns t ON t.id = f.operator_turn_id",
    )
    .fetch_one(&stack.pool)
    .await?;
    let second_request: i64 =
        sqlx::query_scalar("SELECT max(id) FROM requests.requests WHERE human_prompt <> ''")
            .fetch_one(&stack.pool)
            .await?;
    assert_eq!(probe.prompt_kind, "genuine");
    assert_eq!(probe.next_state, "resolved");
    assert_eq!(probe.next_genuine_request_id, Some(second_request));
    assert_eq!(probe.session_class, "operator");
    assert!(
        probe.lag_ms < 20_000,
        "built {} ms after close",
        probe.lag_ms
    );
    let upstream_calls = stack
        .upstream
        .received_requests()
        .await
        .map_or(0, |requests| requests.len());
    assert_eq!(upstream_calls, 2, "the builder must make no HTTP request");
    let llm_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM llm.calls")
        .fetch_one(&stack.pool)
        .await?;
    assert_eq!(llm_rows, 0, "the builder must record no LLM call");
    stack.finish().await
}

#[tokio::test]
async fn builder_toggle_off_writes_nothing_and_on_catches_up() -> anyhow::Result<()> {
    let Some(stack) = Stack::start(Some(10)).await? else {
        return Ok(());
    };
    stack.post_prompt("please refactor the store").await?;
    stack.post_prompt("now write the docs for it").await?;
    let while_off = stack.wait_for_facts(1, Duration::from_secs(5)).await?;
    assert_eq!(
        while_off, 0,
        "the builder is off by default and must write nothing"
    );
    stack.set_toggle(true).await?;
    let caught_up = stack.wait_for_facts(1, Duration::from_secs(40)).await?;
    assert_eq!(
        caught_up, 1,
        "enabling the toggle must catch up within a backstop interval"
    );
    stack.finish().await
}
