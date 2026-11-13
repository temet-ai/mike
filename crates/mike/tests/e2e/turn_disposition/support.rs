use crate::anthropic_fixtures;
use crate::test_db;

use std::time::Duration;

use anthropic_fixtures::{
    MockResponse, RequestBody, assistant_message, tool_result_block, tool_use_block, user_message,
};
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(crate) const ABORT_MARKER: &str = "ABORT-ME-OVERSIZE";
pub(crate) const RESPONSE_CAP_BYTES: u64 = 1024;
pub(crate) const MAIN_SYSTEM: &str = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.";
pub(crate) const WORKFLOW_SYSTEM_CURRENT: &str = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a subagent spawned by a workflow orchestration script.";
pub(crate) const WORKFLOW_HARNESS_PREFIX_PROMPT: &str =
    "[Workflow harness — step 3 of 5] summarise the failing tests";

pub(crate) struct Stack {
    pub(crate) proxy_url: String,
    pub(crate) state: AppState,
    pub(crate) shutdown: CancellationToken,
    pub(crate) pool: PgPool,
    pub(crate) test_db: test_db::TestDbHandle,
    _upstream: MockServer,
}

impl Stack {
    pub(crate) async fn start() -> anyhow::Result<Option<Self>> {
        let Some(factory) = test_db::TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let test_db = factory.acquire_isolated().await?;
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(body_string_contains(ABORT_MARKER))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"type": "message", "role": "assistant", "content": [{"type": "text", "text": "x".repeat(4096)}]})),
            )
            .mount(&upstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(MockResponse::new("msg_disposition").minimal().build()),
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
            "--max-response-body-bytes".to_string(),
            RESPONSE_CAP_BYTES.to_string(),
        ];
        let state = AppState::new(ProxyConfig::try_parse_from(args)?).await?;
        let shutdown = state.shutdown();
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
        let proxy_url = format!("http://{}", proxy_listener.local_addr()?);
        let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
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
        Ok(Some(Self {
            proxy_url,
            state,
            shutdown,
            pool,
            test_db,
            _upstream: upstream,
        }))
    }

    pub(crate) async fn finish(self) -> anyhow::Result<()> {
        self.shutdown.cancel();
        self.pool.close().await;
        self.test_db.cleanup().await?;
        Ok(())
    }

    pub(crate) async fn post(
        &self,
        session_id: &str,
        agent_id: Option<&str>,
        body: &Value,
    ) -> anyhow::Result<u16> {
        let mut req = reqwest::Client::new()
            .post(format!("{}/v1/messages", self.proxy_url))
            .header("x-api-key", "sk-test")
            .header("anthropic-version", "2023-06-01")
            .header("x-claude-code-session-id", session_id)
            .json(body);
        if let Some(agent_id) = agent_id {
            req = req.header("x-claude-code-agent-id", agent_id);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let _ = resp.bytes().await?;
        self.wait_for_in_flight_drain().await?;
        Ok(status)
    }

    async fn wait_for_in_flight_drain(&self) -> anyhow::Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while self.state.in_flight_len() != 0 {
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "in_flight_len stuck at {}",
                self.state.in_flight_len()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        Ok(())
    }
}

pub(crate) fn prompt_body(text: &str) -> Value {
    RequestBody::default()
        .system(json!([{"type": "text", "text": MAIN_SYSTEM}]))
        .content(text)
        .build()
}

pub(crate) fn system_reminder_only_body(inner: &str) -> Value {
    RequestBody::default()
        .system(json!([{"type": "text", "text": MAIN_SYSTEM}]))
        .messages(json!([user_message(json!([
            {"type": "text", "text": format!("<system-reminder>\n{inner}\n</system-reminder>")}
        ]))]))
        .build()
}

pub(crate) fn tool_loop_body() -> Value {
    RequestBody::default()
        .system(json!([{"type": "text", "text": MAIN_SYSTEM}]))
        .messages(json!([
            user_message("run the tests"),
            assistant_message(json!([tool_use_block(
                "toolu_disp1",
                "Bash",
                json!({"command": "ls"})
            )])),
            user_message(json!([tool_result_block("toolu_disp1", "a.rs")])),
        ]))
        .build()
}

pub(crate) fn workflow_agent_body(system: &str, prompt: &str) -> Value {
    RequestBody::default()
        .system(json!([{"type": "text", "text": system}]))
        .content(prompt)
        .build()
}

pub(crate) fn suggestion_probe_body(marker: &str) -> Value {
    RequestBody::default()
        .system(json!([{"type": "text", "text": MAIN_SYSTEM}]))
        .messages(json!([
            user_message("fix the bug"),
            assistant_message("done"),
            user_message(format!(
                "[SUGGESTION MODE: predict the next message] {marker}"
            )),
        ]))
        .build()
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct TurnRow {
    pub(crate) id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) is_open: bool,
    pub(crate) opened_by: Option<String>,
    pub(crate) prompt_kind: Option<String>,
}

pub(crate) async fn turns(pool: &PgPool, session_id: &str) -> anyhow::Result<Vec<TurnRow>> {
    Ok(sqlx::query_as::<_, TurnRow>(
        "SELECT ot.id, ot.end_request_id,
                ot.closed_at IS NULL AS is_open, ot.opened_by,
                k.code AS prompt_kind
           FROM memory.operator_turns ot
           JOIN sessions.sessions s ON s.id = ot.session_id
           LEFT JOIN prompt_kinds.classifications c ON c.request_id = ot.start_request_id
           LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
          WHERE s.session_uuid = $1::uuid
          ORDER BY ot.start_request_id ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?)
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct RequestRow {
    pub(crate) id: i64,
    pub(crate) turn_origin: String,
    pub(crate) author: String,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) status: String,
}

pub(crate) async fn requests(pool: &PgPool, session_id: &str) -> anyhow::Result<Vec<RequestRow>> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let rows = sqlx::query_as::<_, RequestRow>(
            "SELECT r.id, ro.code AS turn_origin, ak.code AS author,
                    r.operator_turn_id, st.code AS status
               FROM requests.requests r
               JOIN sessions.sessions s ON s.id = r.session_id
               JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
               JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
               JOIN requests.statuses st ON st.id = r.status_id
              WHERE s.session_uuid = $1::uuid
              ORDER BY r.id ASC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await?;
        if rows.iter().all(|r| r.status != "pending") || std::time::Instant::now() > deadline {
            return Ok(rows);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub(crate) async fn wait_for_turns(
    pool: &PgPool,
    session_id: &str,
    expected: usize,
) -> anyhow::Result<Vec<TurnRow>> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let rows = turns(pool, session_id).await?;
        if rows.len() >= expected {
            return Ok(rows);
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {expected} turns on {session_id}; saw {rows:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
