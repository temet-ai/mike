use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn diagnostic_stats(&self, project: Option<&str>) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(p) = project {
            query.push(("project", p.to_string()));
        }
        self.get_json("/api/diagnostics/stats", &query).await
    }

    pub(crate) async fn diagnostic_memory(&self, top: Option<u32>) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = Vec::with_capacity(1);
        if let Some(n) = top {
            query.push(("top", n.to_string()));
        }
        self.get_json("/api/diagnostics/memory", &query).await
    }

    pub(crate) async fn health(&self) -> Result<Value, McpError> {
        self.get_json("/api/health", &[]).await
    }

    pub(crate) async fn stats(&self) -> Result<Value, McpError> {
        self.get_json("/api/stats", &[]).await
    }

    pub(crate) async fn combined_stats(&self) -> Result<Value, McpError> {
        let (runtime, diagnostic, health) =
            tokio::try_join!(self.stats(), self.diagnostic_stats(None), self.health())?;
        Ok(serde_json::json!({
            "runtime": runtime,
            "diagnostic": diagnostic,
            "health": health,
        }))
    }

    pub(crate) async fn diagnostic_findings(
        &self,
        order: Option<&str>,
        project: Option<&str>,
    ) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(ord) = order {
            query.push(("order", ord.to_string()));
        }
        if let Some(p) = project {
            query.push(("project", p.to_string()));
        }
        self.get_json("/api/diagnostics/findings", &query).await
    }

    pub(crate) async fn get_verifier_rate(&self, project: Option<&str>) -> Result<Value, McpError> {
        let mut q: Vec<(&'static str, String)> = Vec::new();
        if let Some(p) = project {
            q.push(("project", p.to_string()));
        }
        self.get_json("/api/steering/verifier-rate", &q).await
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_mock() -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    #[tokio::test]
    async fn combined_stats_merges_runtime_diagnostic_and_health() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "request_count": 7,
                "active_streams": 0,
                "uptime_seconds": 12
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/diagnostics/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "totals_by_category": {"Clarity": 2},
                "totals_by_analyzer": {"weasel.placeholder": 2},
                "last_24h": 2
            })))
            .mount(&server)
            .await;

        let value = client.combined_stats().await?;
        assert_eq!(value["runtime"]["request_count"], 7);
        assert_eq!(value["diagnostic"]["last_24h"], 2);
        assert_eq!(value["health"]["status"], "ok");
        Ok(())
    }

    #[tokio::test]
    async fn combined_stats_returns_error_when_any_sub_fetch_fails() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "request_count": 0
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/diagnostics/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "last_24h": 0
            })))
            .mount(&server)
            .await;

        let err = client
            .combined_stats()
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected error on missing /api/health mock"))?;
        match err {
            McpError::UpstreamStatus { status, .. } => assert_eq!(status, 404),
            other => anyhow::bail!("expected UpstreamStatus 404, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn health_and_stats_and_diagnostic_stats_hit_right_paths() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "request_count": 3
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/diagnostics/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "totals_by_category": {}
            })))
            .mount(&server)
            .await;

        let h = client.health().await?;
        assert_eq!(h["status"], "ok");
        let s = client.stats().await?;
        assert_eq!(s["request_count"], 3);
        let d = client.diagnostic_stats(None).await?;
        assert!(d["totals_by_category"].is_object());
        Ok(())
    }

    #[tokio::test]
    async fn diagnostic_findings_returns_array_passthrough() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/diagnostics/findings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"analyzer": "weasel.placeholder", "request_id": 7}
            ])))
            .mount(&server)
            .await;
        let value = client.diagnostic_findings(None, None).await?;
        assert_eq!(value[0]["request_id"], 7);
        Ok(())
    }
}
