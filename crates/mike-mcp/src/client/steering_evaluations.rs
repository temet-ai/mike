use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

#[derive(Debug, Default)]
pub(crate) struct SteererEvaluationsQuery<'a> {
    pub(crate) steerer_id: Option<i64>,
    pub(crate) question: Option<&'a str>,
    pub(crate) mode: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) limit: Option<i64>,
    pub(crate) before_id: Option<i64>,
}

impl SteererEvaluationsQuery<'_> {
    fn pairs(&self) -> Vec<(&'static str, String)> {
        let texts = [
            ("question", self.question),
            ("mode", self.mode),
            ("status", self.status),
        ];
        let numbers = [
            ("steerer_id", self.steerer_id),
            ("limit", self.limit),
            ("before_id", self.before_id),
        ];
        texts
            .into_iter()
            .filter_map(|(name, value)| value.map(|v| (name, v.to_owned())))
            .chain(
                numbers
                    .into_iter()
                    .filter_map(|(name, value)| value.map(|v| (name, v.to_string()))),
            )
            .collect()
    }
}

impl InspectClient {
    pub(crate) async fn list_steerer_evaluations(
        &self,
        session: &str,
        query: &SteererEvaluationsQuery<'_>,
    ) -> Result<Value, McpError> {
        self.get_json(
            &format!("/api/sessions/{session}/evaluations"),
            &query.pairs(),
        )
        .await
    }
}
