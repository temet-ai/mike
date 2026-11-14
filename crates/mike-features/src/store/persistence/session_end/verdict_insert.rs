use chrono::{DateTime, Utc};

use super::super::PersistError;

const INSERT_SQL: &str = "INSERT INTO session_judges.verdicts
    (session_id, outcome_id, quality_score, rationale, judge_model, rubric_version, judged_at, mode,
     bundle_id, rubric_id, end_turn_id, end_label, end_source, end_rule, end_confidence, end_evidence,
     failure, failure_tags, severity, slices, as_of, digest_version, input_text, llm_output,
     generation_id, cost_usd, prompt_tokens, completion_tokens, latency_ms)
VALUES ($1, $2, $3::numeric(4,3), $4, $5, $6, now(), $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
        $19, $20, $21, $22, $23, $24, $25, $26, $27, $28)
RETURNING id";

#[derive(Debug, Clone, PartialEq)]
pub struct SessionVerdictV3Row<'a> {
    pub session_db_id: i64,
    pub outcome_id: i64,
    pub quality_score: Option<f64>,
    pub rationale: &'a str,
    pub judge_model: &'a str,
    pub rubric_version: i32,
    pub mode: &'a str,
    pub bundle_id: Option<i64>,
    pub rubric_id: &'a str,
    pub end_turn_id: Option<i64>,
    pub end_label: Option<&'a str>,
    pub end_source: Option<&'a str>,
    pub end_rule: Option<&'a str>,
    pub end_confidence: Option<&'a str>,
    pub end_evidence: Option<&'a str>,
    pub failure: Option<&'a str>,
    pub failure_tags: Vec<String>,
    pub severity: Option<&'a str>,
    pub slices: Vec<String>,
    pub as_of: DateTime<Utc>,
    pub digest_version: &'a str,
    pub input_text: &'a str,
    pub llm_output: serde_json::Value,
    pub generation_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn insert_session_verdict_v3(
        self,
        r: &SessionVerdictV3Row<'_>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_session_verdict_v3", async move {
            Ok(sqlx::query_scalar(INSERT_SQL)
                .bind(r.session_db_id)
                .bind(r.outcome_id)
                .bind(r.quality_score)
                .bind(r.rationale)
                .bind(r.judge_model)
                .bind(r.rubric_version)
                .bind(r.mode)
                .bind(r.bundle_id)
                .bind(r.rubric_id)
                .bind(r.end_turn_id)
                .bind(r.end_label)
                .bind(r.end_source)
                .bind(r.end_rule)
                .bind(r.end_confidence)
                .bind(r.end_evidence)
                .bind(r.failure)
                .bind(&r.failure_tags)
                .bind(r.severity)
                .bind(&r.slices)
                .bind(r.as_of)
                .bind(r.digest_version)
                .bind(r.input_text)
                .bind(&r.llm_output)
                .bind(&r.generation_id)
                .bind(r.cost_usd)
                .bind(r.prompt_tokens)
                .bind(r.completion_tokens)
                .bind(r.latency_ms)
                .fetch_one(&self.pool)
                .await?)
        })
        .await
    }
}
