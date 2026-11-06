use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

#[derive(Debug, Default)]
pub(crate) struct ConfigChangesQuery<'a> {
    pub(crate) subject_kind: Option<&'a str>,
    pub(crate) subject_key: Option<&'a str>,
    pub(crate) subject_key_prefix: Option<&'a str>,
    pub(crate) via: Option<&'a str>,
    pub(crate) since: Option<&'a str>,
    pub(crate) until: Option<&'a str>,
    pub(crate) before_id: Option<i64>,
    pub(crate) limit: Option<i64>,
}

impl ConfigChangesQuery<'_> {
    fn pairs(&self) -> Vec<(&'static str, String)> {
        let texts = [
            ("subject_kind", self.subject_kind),
            ("subject_key", self.subject_key),
            ("subject_key_prefix", self.subject_key_prefix),
            ("via", self.via),
            ("since", self.since),
            ("until", self.until),
        ];
        let mut pairs: Vec<(&'static str, String)> = texts
            .into_iter()
            .filter_map(|(name, value)| value.map(|v| (name, v.to_owned())))
            .collect();
        if let Some(before_id) = self.before_id {
            pairs.push(("before_id", before_id.to_string()));
        }
        if let Some(limit) = self.limit {
            pairs.push(("limit", limit.to_string()));
        }
        pairs
    }
}

impl InspectClient {
    pub(crate) async fn list_config_changes(
        &self,
        query: &ConfigChangesQuery<'_>,
    ) -> Result<Value, McpError> {
        self.get_json("/api/config/change-log", &query.pairs())
            .await
    }

    pub(crate) async fn get_config_snapshot(
        &self,
        at: &str,
        subject_kind: Option<&str>,
    ) -> Result<Value, McpError> {
        let mut pairs = vec![("at", at.to_owned())];
        if let Some(kind) = subject_kind {
            pairs.push(("subject_kind", kind.to_owned()));
        }
        self.get_json("/api/config/snapshot", &pairs).await
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn list_config_changes_forwards_filters_and_returns_the_body() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        let body = serde_json::json!({"changes": [{"id": 7}], "next_before_id": null});
        Mock::given(method("GET"))
            .and(path("/api/config/change-log"))
            .and(query_param("via", "sql"))
            .and(query_param("limit", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;
        let client = InspectClient::new(server.uri());
        let query = ConfigChangesQuery {
            via: Some("sql"),
            limit: Some(5),
            ..ConfigChangesQuery::default()
        };
        assert_eq!(client.list_config_changes(&query).await?, body);
        Ok(())
    }

    #[tokio::test]
    async fn get_config_snapshot_sends_at() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        let body = serde_json::json!({"at": "2026-09-24T00:00:00Z", "entries": []});
        Mock::given(method("GET"))
            .and(path("/api/config/snapshot"))
            .and(query_param("at", "2026-09-24T00:00:00Z"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;
        let client = InspectClient::new(server.uri());
        assert_eq!(
            client
                .get_config_snapshot("2026-09-24T00:00:00Z", None)
                .await?,
            body
        );
        Ok(())
    }
}
