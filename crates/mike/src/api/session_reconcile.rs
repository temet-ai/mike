use std::path::PathBuf;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AppState;

const DEFAULT_TETHER_URL: &str = "http://127.0.0.1:9970";
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Usage {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cache_read_input_tokens: u64,
    pub(crate) cache_creation_input_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Step {
    pub(crate) turn: String,
    pub(crate) index: u32,
    pub(crate) model: String,
    pub(crate) agent: Option<String>,
    pub(crate) usage: Option<Usage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WireRequest {
    pub(crate) request_id: String,
    pub(crate) origin: String,
    pub(crate) model: Option<String>,
    pub(crate) usage: Option<Usage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Report {
    pub(crate) session: String,
    pub(crate) continues: Option<String>,
    pub(crate) mod_steps: usize,
    pub(crate) wire_requests: usize,
    pub(crate) matched: usize,
    pub(crate) only_in_mod: Vec<Step>,
    pub(crate) only_on_wire: Vec<WireRequest>,
    #[serde(default)]
    pub(crate) only_on_wire_by_origin: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReconcileAnswer {
    pub(crate) available: bool,
    pub(crate) reason: Option<String>,
    pub(crate) report: Option<Report>,
}

impl ReconcileAnswer {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
            report: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TetherEndpoint {
    pub(crate) base: String,
    pub(crate) token_path: Option<PathBuf>,
}

impl TetherEndpoint {
    pub(crate) fn from_env() -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        let base = var("TETHER_URL")
            .and_then(|v| v.into_string().ok())
            .unwrap_or_else(|| DEFAULT_TETHER_URL.to_owned());
        let dir = var("TETHER_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| var("XDG_CONFIG_HOME").map(|x| PathBuf::from(x).join("tether")))
            .or_else(|| var("HOME").map(|h| PathBuf::from(h).join(".config/tether")));
        Self {
            base,
            token_path: dir.map(|d| d.join("operator-token")),
        }
    }
}

pub(crate) async fn get_session_reconcile(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReconcileAnswer>, StatusCode> {
    let Ok(session) = Uuid::parse_str(&id) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    Ok(Json(fetch(&TetherEndpoint::from_env(), session).await))
}

pub(crate) async fn fetch(endpoint: &TetherEndpoint, session: Uuid) -> ReconcileAnswer {
    let token = endpoint
        .token_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty());
    let Some(token) = token else {
        return ReconcileAnswer::unavailable(
            "tether's operator token was not found, so mike cannot read its reconciliation",
        );
    };
    let Ok(client) = reqwest::Client::builder().timeout(TIMEOUT).build() else {
        return ReconcileAnswer::unavailable("mike could not build an HTTP client for tether");
    };
    let url = format!(
        "{}/v1/features/reconcile/sessions/{session}",
        endpoint.base.trim_end_matches('/')
    );
    let response = match client.get(&url).bearer_auth(token).send().await {
        Ok(response) => response,
        Err(e) if e.is_connect() => {
            return ReconcileAnswer::unavailable(format!(
                "tether is not running at {}",
                endpoint.base
            ));
        }
        Err(e) => return ReconcileAnswer::unavailable(format!("tether did not answer: {e}")),
    };
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return ReconcileAnswer::unavailable(format!("tether answered {status}: {}", body.trim()));
    }
    match response.json::<Report>().await {
        Ok(report) => ReconcileAnswer {
            available: true,
            reason: None,
            report: Some(report),
        },
        Err(e) => ReconcileAnswer::unavailable(format!("tether's report did not read back: {e}")),
    }
}

#[cfg(test)]
mod tests;
