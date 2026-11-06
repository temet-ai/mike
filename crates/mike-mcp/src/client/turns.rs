use serde_json::Value;

use super::InspectClient;
use super::SessionOverviewQuery;
use super::http::build_segments_url;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn get_operator_turn(&self, id: i64) -> Result<Value, McpError> {
        let path = format!("/api/operator-turns/{id}");
        self.get_json_with_not_found(&path, &format!("operator turn id {id}"))
            .await
    }

    pub(crate) async fn get_session_operator_turns(
        &self,
        session_id: &str,
        order: Option<&str>,
        prompt_kind: Option<&str>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Value, McpError> {
        let mut url = build_segments_url(
            &self.base_url,
            &["api", "sessions", session_id, "operator-turns"],
        )?;
        {
            let mut q = url.query_pairs_mut();
            if let Some(ord) = order {
                q.append_pair("order", ord);
            }
            if let Some(pk) = prompt_kind {
                q.append_pair("prompt_kind", pk);
            }
            if let Some(l) = limit {
                q.append_pair("limit", &l.to_string());
            }
            if let Some(o) = offset {
                q.append_pair("offset", &o.to_string());
            }
        }
        self.get_json_full_url(url, &format!("session id {session_id}"))
            .await
    }

    pub(crate) async fn get_session_overview(
        &self,
        session_id: &str,
        filter: SessionOverviewQuery<'_>,
    ) -> Result<Value, McpError> {
        let mut url =
            build_segments_url(&self.base_url, &["api", "sessions", session_id, "overview"])?;
        {
            let mut q = url.query_pairs_mut();
            if let Some(ord) = filter.order {
                q.append_pair("order", ord);
            }
            if let Some(pk) = filter.prompt_kind {
                q.append_pair("prompt_kind", pk);
            }
            if let Some(l) = filter.limit {
                q.append_pair("limit", &l.to_string());
            }
            if let Some(o) = filter.offset {
                q.append_pair("offset", &o.to_string());
            }
            if let Some(f) = filter.fields {
                q.append_pair("fields", f);
            }
        }
        self.get_json_full_url(url, &format!("session id {session_id}"))
            .await
    }

    pub(crate) async fn list_outcome_summary(
        &self,
        project: Option<&str>,
        since: Option<&str>,
    ) -> Result<Value, McpError> {
        let mut q: Vec<(&'static str, String)> = Vec::new();
        if let Some(p) = project {
            q.push(("project", p.to_string()));
        }
        if let Some(s) = since {
            q.push(("since", s.to_string()));
        }
        self.get_json("/api/operator-turns/outcome-summary", &q)
            .await
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_mock() -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    #[tokio::test]
    async fn get_session_operator_turns_forwards_order() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/sessions/sess-turns/operator-turns"))
            .and(query_param("order", "asc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let value = client
            .get_session_operator_turns("sess-turns", Some("asc"), None, None, None)
            .await?;
        assert!(value.is_array());
        Ok(())
    }
}
