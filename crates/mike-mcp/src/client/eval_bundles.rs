use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

#[derive(Debug, Default)]
pub(crate) struct BundlesQuery<'a> {
    pub(crate) role: Option<&'a str>,
    pub(crate) subject_key: Option<&'a str>,
    pub(crate) origin: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) current: Option<bool>,
    pub(crate) before_id: Option<i64>,
    pub(crate) limit: Option<i64>,
}

impl BundlesQuery<'_> {
    fn pairs(&self) -> Vec<(&'static str, String)> {
        let texts = [
            ("role", self.role),
            ("subject_key", self.subject_key),
            ("origin", self.origin),
            ("status", self.status),
        ];
        let mut pairs: Vec<(&'static str, String)> = texts
            .into_iter()
            .filter_map(|(name, value)| value.map(|v| (name, v.to_owned())))
            .collect();
        if let Some(current) = self.current {
            pairs.push(("current", current.to_string()));
        }
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
    pub(crate) async fn list_eval_bundles(
        &self,
        query: &BundlesQuery<'_>,
    ) -> Result<Value, McpError> {
        self.get_json("/api/eval/bundles", &query.pairs()).await
    }

    pub(crate) async fn get_eval_bundle(&self, id: i64) -> Result<Value, McpError> {
        self.get_json_with_not_found(&format!("/api/eval/bundles/{id}"), &format!("bundle {id}"))
            .await
    }
}
