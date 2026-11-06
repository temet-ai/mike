use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

#[derive(Debug, Default)]
pub(crate) struct TurnVerdictsQuery<'a> {
    pub(crate) mode: Option<&'a str>,
    pub(crate) bundle_id: Option<i64>,
    pub(crate) rubric_version: Option<&'a str>,
    pub(crate) session_id: Option<&'a str>,
    pub(crate) since: Option<&'a str>,
    pub(crate) until: Option<&'a str>,
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

impl TurnVerdictsQuery<'_> {
    fn pairs(&self) -> Vec<(&'static str, String)> {
        let texts = [
            ("mode", self.mode),
            ("rubric_version", self.rubric_version),
            ("session_id", self.session_id),
            ("since", self.since),
            ("until", self.until),
        ];
        let numbers = [
            ("bundle_id", self.bundle_id),
            ("limit", self.limit),
            ("offset", self.offset),
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
    pub(crate) async fn get_turn_verdicts(
        &self,
        turn_id: i64,
        query: &TurnVerdictsQuery<'_>,
    ) -> Result<Value, McpError> {
        self.get_json_with_not_found_query(
            &format!("/api/operator-turns/{turn_id}/verdicts"),
            &query.pairs(),
            &format!("operator turn {turn_id}"),
        )
        .await
    }

    pub(crate) async fn list_turn_verdicts(
        &self,
        query: &TurnVerdictsQuery<'_>,
    ) -> Result<Value, McpError> {
        self.get_json("/api/turn-verdicts", &query.pairs()).await
    }
}
