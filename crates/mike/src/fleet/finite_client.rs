use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum FiniteClientError {
    #[error("finite request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("finite returned status {status} for run {run_id}")]
    Status {
        status: reqwest::StatusCode,
        run_id: String,
    },
    #[error("finite returned status {status} for the run list")]
    ListStatus { status: reqwest::StatusCode },
    #[error("failed to deserialize finite response: {0}")]
    Deserialize(#[source] reqwest::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LedgerEntry {
    pub(crate) event_kind: String,
    #[serde(default)]
    pub(crate) actor_id: Option<String>,
    #[serde(default)]
    pub(crate) ts: Option<String>,
    #[serde(default)]
    pub(crate) verdict: Option<String>,
    #[serde(default)]
    pub(crate) payload: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RunManifest {
    pub(crate) run_id: String,
    #[serde(default)]
    pub(crate) entries: Vec<LedgerEntry>,
}

pub(crate) async fn fetch_run_manifest(
    client: &reqwest::Client,
    base_url: &str,
    run_id: &str,
) -> Result<RunManifest, FiniteClientError> {
    let url = format!(
        "{}/api/run/{run_id}/manifest",
        base_url.trim_end_matches('/')
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(FiniteClientError::Request)?;

    if !resp.status().is_success() {
        return Err(FiniteClientError::Status {
            status: resp.status(),
            run_id: run_id.to_string(),
        });
    }

    resp.json::<RunManifest>()
        .await
        .map_err(FiniteClientError::Deserialize)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RunSummary {
    #[serde(default)]
    pub(crate) run_id: String,
    #[serde(default)]
    pub(crate) machine_id: String,
    #[serde(default)]
    pub(crate) latest_ts: String,
    #[serde(default)]
    pub(crate) terminal: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RunsListResponse {
    #[serde(default)]
    runs: Vec<RunSummary>,
}

pub(crate) async fn fetch_runs_list(
    client: &reqwest::Client,
    base_url: &str,
) -> Result<Vec<RunSummary>, FiniteClientError> {
    let url = format!("{}/api/runs", base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(FiniteClientError::Request)?;

    if !resp.status().is_success() {
        return Err(FiniteClientError::ListStatus {
            status: resp.status(),
        });
    }

    let parsed = resp
        .json::<RunsListResponse>()
        .await
        .map_err(FiniteClientError::Deserialize)?;
    Ok(parsed.runs)
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn fetch_run_manifest_parses_entries() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/run/run-1/manifest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "run_id": "run-1",
                "entries": [
                    {
                        "id": 1,
                        "run_id": "run-1",
                        "event_kind": "turn-report",
                        "payload": {"spawn_ack": true, "parent_node_id": "actor-1", "session_uuid": "11111111-1111-1111-1111-111111111111"}
                    }
                ]
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let manifest = fetch_run_manifest(&client, &server.uri(), "run-1").await?;
        assert_eq!(manifest.run_id, "run-1");
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].event_kind, "turn-report");
        Ok(())
    }

    #[tokio::test]
    async fn fetch_run_manifest_non_success_status_is_err() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/run/run-missing/manifest"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        match fetch_run_manifest(&client, &server.uri(), "run-missing").await {
            Err(FiniteClientError::Status { .. }) => {}
            other => panic!("404 must surface as FiniteClientError::Status, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn fetch_runs_list_parses_runs() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "runs": [
                    {
                        "run_id": "run-2",
                        "machine_id": "machine-a",
                        "latest_ts": "2026-07-12T18:50:31.071Z",
                        "terminal": true
                    },
                    {
                        "run_id": "run-1",
                        "machine_id": "machine-b",
                        "latest_ts": "2026-07-12T18:40:00.000Z",
                        "terminal": false
                    }
                ]
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let runs = fetch_runs_list(&client, &server.uri()).await?;
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].run_id, "run-2");
        assert_eq!(runs[0].machine_id, "machine-a");
        assert!(runs[0].terminal);
        assert_eq!(runs[1].run_id, "run-1");
        assert!(!runs[1].terminal);
        Ok(())
    }

    #[tokio::test]
    async fn fetch_runs_list_non_success_status_is_err() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/runs"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        match fetch_runs_list(&client, &server.uri()).await {
            Err(FiniteClientError::ListStatus { .. }) => {}
            other => panic!("503 must surface as FiniteClientError::ListStatus, got {other:?}"),
        }
        Ok(())
    }
}
