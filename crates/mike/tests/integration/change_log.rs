use mike::config::{AppState, ProxyConfig};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpListener;

use crate::test_db::{TestDb, TestDbHandle};

#[path = "change_log/builds.rs"]
mod builds;
#[path = "change_log/rows.rs"]
mod rows;
#[path = "change_log/snapshot.rs"]
mod snapshot;

pub(crate) const SPA_ORIGIN: &str = "http://127.0.0.1:9990";

pub(crate) struct Stack {
    pub(crate) base: String,
    pub(crate) state: AppState,
    pub(crate) raw: PgPool,
    pub(crate) db: TestDbHandle,
}

impl Stack {
    pub(crate) async fn start() -> anyhow::Result<Option<Self>> {
        let Some(factory) = TestDb::bootstrap().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping change_log test");
            return Ok(None);
        };
        let db = factory.acquire_isolated().await?;
        let args = vec![
            "mike".to_string(),
            "--inspect-listen-addr".to_string(),
            "127.0.0.1:0".to_string(),
            "--persist-enabled".to_string(),
            "1".to_string(),
            "--database-url".to_string(),
            db.database_url.clone(),
        ];
        let state = AppState::new(ProxyConfig::try_parse_from(args)?).await?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}", listener.local_addr()?);
        let router = mike::app::inspect_router_on(state.clone(), listener.local_addr()?);
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let raw = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&db.database_url)
            .await?;
        Ok(Some(Self {
            base,
            state,
            raw,
            db,
        }))
    }

    pub(crate) async fn finish(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.raw.close().await;
        self.db.cleanup().await?;
        Ok(())
    }

    pub(crate) async fn patch(
        &self,
        route: &str,
        body: Value,
        origin: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut request = reqwest::Client::new()
            .patch(format!("{}{route}", self.base))
            .json(&body);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = request.send().await?;
        let status = response.status();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        anyhow::ensure!(status.is_success(), "PATCH {route}: {status}: {value}");
        Ok(value)
    }

    pub(crate) async fn get(&self, route: &str) -> anyhow::Result<Value> {
        let response = reqwest::Client::new()
            .get(format!("{}{route}", self.base))
            .send()
            .await?;
        let status = response.status();
        let value: Value = response.json().await?;
        anyhow::ensure!(status.is_success(), "GET {route}: {status}: {value}");
        Ok(value)
    }

    pub(crate) async fn rows_for(&self, kind: &str, key: &str) -> anyhow::Result<Vec<Value>> {
        let page = self
            .get(&format!(
                "/api/config/change-log?subject_kind={kind}&subject_key={key}&limit=500"
            ))
            .await?;
        Ok(page["changes"].as_array().cloned().unwrap_or_default())
    }
}

pub(crate) fn non_observe(rows: &[Value]) -> Vec<&Value> {
    rows.iter()
        .filter(|row| row["op"] != json!("observe"))
        .collect()
}
