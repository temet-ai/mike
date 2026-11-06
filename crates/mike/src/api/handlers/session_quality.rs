use crate::store::persistence::judge::SessionQualityRow;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SessionQualityDto {
    pub(crate) outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quality_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rationale: Option<String>,
    pub(crate) judge_model: String,
    pub(crate) rubric_version: i32,
    pub(crate) judged_at: String,
    pub(crate) bundle_id: Option<i64>,
    pub(crate) verdict_id: i64,
    pub(crate) end_turn_id: Option<i64>,
    pub(crate) end_label: Option<String>,
    pub(crate) end_source: Option<String>,
    pub(crate) end_rule: Option<String>,
    pub(crate) end_confidence: Option<String>,
    pub(crate) end_evidence: Option<String>,
    pub(crate) failure: Option<String>,
    pub(crate) failure_tags: Vec<String>,
    pub(crate) severity: Option<String>,
}

impl From<SessionQualityRow> for SessionQualityDto {
    fn from(q: SessionQualityRow) -> Self {
        Self {
            outcome: q.outcome,
            quality_score: q.quality_score,
            rationale: q.rationale,
            judge_model: q.judge_model,
            rubric_version: q.rubric_version,
            judged_at: q.judged_at.to_rfc3339(),
            bundle_id: q.bundle_id,
            verdict_id: q.verdict_id,
            end_turn_id: q.end_turn_id,
            end_label: q.end_label,
            end_source: q.end_source,
            end_rule: q.end_rule,
            end_confidence: q.end_confidence,
            end_evidence: q.end_evidence,
            failure: q.failure,
            failure_tags: q.failure_tags,
            severity: q.severity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_quality_dto_carries_the_end_fields_whole() -> anyhow::Result<()> {
        let evidence = "operator: this is going nowhere, closing it. ".repeat(40);
        let row = SessionQualityRow {
            outcome: "failure".to_owned(),
            quality_score: Some(0.2),
            rationale: Some("stalled".to_owned()),
            judge_model: "m".to_owned(),
            rubric_version: 3,
            judged_at: chrono::Utc::now(),
            bundle_id: Some(7),
            verdict_id: 11,
            end_turn_id: Some(42),
            end_label: Some("closed_in_frustration".to_owned()),
            end_source: Some("llm".to_owned()),
            end_rule: None,
            end_confidence: Some("high".to_owned()),
            end_evidence: Some(evidence.clone()),
            failure: Some("looped".to_owned()),
            failure_tags: vec!["looping".to_owned()],
            severity: Some("major".to_owned()),
        };
        let json = serde_json::to_value(SessionQualityDto::from(row))?;
        assert_eq!(json["end_label"], "closed_in_frustration");
        assert_eq!(json["end_turn_id"], 42);
        assert_eq!(json["end_evidence"].as_str(), Some(evidence.as_str()));
        assert_eq!(json["verdict_id"], 11);
        Ok(())
    }
}
