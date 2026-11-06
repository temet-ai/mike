#[path = "eval_labels/import.rs"]
mod import;
#[path = "eval_labels/rest.rs"]
mod rest;

use mike::config::{AppState, ProxyConfig};
use serde_json::Value;
use tokio::net::TcpListener;

use crate::turn_facts::Fixture;

pub(crate) const SPA: &str = "http://127.0.0.1:9990";

pub(crate) struct Api {
    pub(crate) base: String,
    pub(crate) state: AppState,
}

pub(crate) async fn api(f: &Fixture) -> anyhow::Result<Api> {
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
    Ok(Api { base, state })
}

impl Api {
    pub(crate) async fn post(
        &self,
        route: &str,
        body: Value,
        origin: Option<&str>,
    ) -> anyhow::Result<(u16, Value)> {
        let mut request = reqwest::Client::new()
            .post(format!("{}{route}", self.base))
            .json(&body);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        Ok((status, response.json().await.unwrap_or(Value::Null)))
    }

    pub(crate) fn stop(&self) {
        self.state.shutdown().cancel();
    }
}
