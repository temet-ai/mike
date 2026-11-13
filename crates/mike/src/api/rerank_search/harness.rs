use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

pub(crate) struct Harness {
    pub(crate) base: String,
    pub(crate) client: reqwest::Client,
    pub(crate) state: AppState,
    pub(crate) handle: PersistHandle,
    pub(crate) reranker: MockServer,
    db: TestDbHandle,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Harness {
    pub(crate) async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        state.persist = Some(handle.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let router = crate::app::inspect_router_on(state.clone(), listener.local_addr()?);
        let base = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Self {
            base,
            client: reqwest::Client::new(),
            state,
            handle,
            reranker: MockServer::start().await,
            db,
            server,
        })
    }

    pub(crate) fn enable_rerank(&self, timeout_ms: u64) {
        let service = self.state.rerank_service();
        service.apply_setting("rerank.url", &json!(self.reranker.uri()));
        service.apply_setting("rerank.timeout_ms", &json!(timeout_ms));
        service.apply_setting("rerank.cold_timeout_ms", &json!(timeout_ms));
        service.apply_setting("rerank.enabled", &json!(true));
    }

    pub(crate) async fn reranker_prefers_later_documents(&self, times: Option<u64>) {
        let mock = Mock::given(method("POST"))
            .and(path("/v1/rerank"))
            .respond_with(score_by_position);
        match times {
            Some(times) => mock.up_to_n_times(times).mount(&self.reranker).await,
            None => mock.mount(&self.reranker).await,
        }
    }

    pub(crate) async fn reranker_answers(&self, template: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/v1/rerank"))
            .respond_with(template)
            .mount(&self.reranker)
            .await;
    }

    pub(crate) async fn rerank_requests(&self) -> anyhow::Result<Vec<Value>> {
        let requests = self
            .reranker
            .received_requests()
            .await
            .ok_or_else(|| anyhow::anyhow!("request recording is off"))?;
        Ok(requests
            .iter()
            .filter(|r| r.url.path() == "/v1/rerank")
            .filter_map(|r| serde_json::from_slice(&r.body).ok())
            .collect())
    }

    pub(crate) async fn get(&self, route: &str) -> anyhow::Result<(StatusCode, Value)> {
        let response = self
            .client
            .get(format!("{}{route}", self.base))
            .send()
            .await?;
        Ok((response.status(), response.json().await?))
    }

    pub(crate) async fn post(
        &self,
        route: &str,
        body: Value,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let response = self
            .client
            .post(format!("{}{route}", self.base))
            .json(&body)
            .send()
            .await?;
        Ok((response.status(), response.json().await?))
    }

    pub(crate) async fn settled(&self) {
        self.state.tracker.close();
        self.state.tracker.wait().await;
        self.state.tracker.reopen();
    }

    pub(crate) async fn events(&self) -> anyhow::Result<Vec<EventRow>> {
        self.settled().await;
        Ok(sqlx::query_as(
            "SELECT tool, query, rerank_state, returned, session_id
               FROM memory.search_events ORDER BY id",
        )
        .fetch_all(self.handle.pool())
        .await?)
    }

    pub(crate) async fn cleanup(self) -> anyhow::Result<()> {
        self.server.abort();
        self.handle.pool().close().await;
        self.db.cleanup().await
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct EventRow {
    pub(crate) tool: String,
    pub(crate) query: String,
    pub(crate) rerank_state: String,
    pub(crate) returned: Value,
    pub(crate) session_id: Option<i64>,
}

fn score_by_position(request: &Request) -> ResponseTemplate {
    let documents = serde_json::from_slice::<Value>(&request.body)
        .ok()
        .and_then(|body| body["documents"].as_array().map(Vec::len))
        .unwrap_or(0);
    let results: Vec<Value> = (0..documents)
        .map(|index| {
            let rank = f64::from(u32::try_from(index + 1).unwrap_or(u32::MAX));
            json!({"index": index, "relevance_score": rank / 100.0})
        })
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({"results": results}))
}

pub(crate) fn slow_answer(delay: Duration) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_delay(delay)
        .set_body_json(json!({"results": []}))
}

pub(crate) fn loading_answer() -> ResponseTemplate {
    ResponseTemplate::new(503).set_body_json(
        json!({"error": {"code": 503, "message": "Loading model", "type": "unavailable_error"}}),
    )
}

pub(crate) fn failing_answer() -> ResponseTemplate {
    ResponseTemplate::new(500).set_body_string("reranker crashed")
}
