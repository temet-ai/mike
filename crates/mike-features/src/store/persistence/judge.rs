use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::error::PersistError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionForJudgment {
    pub session_db_id: i64,
    pub session_id: String,
    pub needs_verdict: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JudgeDigestRow {
    pub request_id: i64,
    pub user_prompt: String,
    pub stop_response: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionQualityRow {
    pub outcome: String,
    pub quality_score: Option<f64>,
    pub rationale: Option<String>,
    pub judge_model: String,
    pub rubric_version: i32,
    pub judged_at: DateTime<Utc>,
    pub bundle_id: Option<i64>,
    pub verdict_id: i64,
    pub end_turn_id: Option<i64>,
    pub end_label: Option<String>,
    pub end_source: Option<String>,
    pub end_rule: Option<String>,
    pub end_confidence: Option<String>,
    pub end_evidence: Option<String>,
    pub failure: Option<String>,
    pub failure_tags: Vec<String>,
    pub severity: Option<String>,
}

pub use crate::settings::meta_session_end::DEFAULT_MAX_ATTEMPTS_PER_DAY;

#[derive(Debug, Clone, Copy)]
pub struct NeedsJudgmentQuery<'a> {
    pub judge_model: &'a str,
    pub rubric_version: i32,
    pub settled_before: DateTime<Utc>,
    pub limit: i64,
    pub max_attempts_per_day: i64,
}

pub async fn fetch_sessions_needing_judgment(
    pool: &PgPool,
    query: &NeedsJudgmentQuery<'_>,
) -> Result<Vec<SessionForJudgment>, PersistError> {
    let rows = sqlx::query_file_as!(
        SessionForJudgment,
        "src/store/persistence/judge/needing_judgment.sql",
        query.settled_before,
        query.judge_model,
        query.rubric_version,
        query.limit,
        query.max_attempts_per_day,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn fetch_judge_digest(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Vec<JudgeDigestRow>, PersistError> {
    let rows = sqlx::query_as!(
        JudgeDigestRow,
        r#"SELECT ot.start_request_id AS request_id,
                  ot.user_prompt AS "user_prompt!",
                  ot.final_assistant_text AS stop_response
             FROM memory.operator_turns ot
            WHERE ot.session_id = $1
            ORDER BY ot.start_request_id ASC"#,
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub struct SessionVerdictInsert<'a> {
    pub session_db_id: i64,
    pub outcome_id: i64,
    pub quality_score: Option<f64>,
    pub rationale: Option<&'a str>,
    pub judge_model: &'a str,
    pub rubric_version: i32,
    pub judged_at: DateTime<Utc>,
    pub bundle_id: Option<i64>,
}

pub async fn insert_session_verdict(
    pool: &PgPool,
    params: SessionVerdictInsert<'_>,
) -> Result<i64, PersistError> {
    let id = sqlx::query_scalar!(
        "INSERT INTO session_judges.verdicts
             (session_id, outcome_id, quality_score, rationale, judge_model, rubric_version, judged_at,
              bundle_id)
         VALUES ($1, $2, $3::float8::numeric(4,3), $4, $5, $6, $7, $8)
         RETURNING id",
        params.session_db_id,
        params.outcome_id,
        params.quality_score,
        params.rationale,
        params.judge_model,
        params.rubric_version,
        params.judged_at,
        params.bundle_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn record_judge_attempt(
    pool: &PgPool,
    session_db_id: i64,
    judge_model: &str,
    rubric_version: i32,
    attempted_at: DateTime<Utc>,
    failed: bool,
) -> Result<i32, PersistError> {
    let window_attempts = sqlx::query_scalar!(
        "INSERT INTO session_judges.attempts
             (session_id, judge_model, rubric_version, last_attempt_at,
              attempt_count, consecutive_failures, window_started_at, window_attempts)
         VALUES ($1, $2, $3, $4, 1, CASE WHEN $5::boolean THEN 1 ELSE 0 END, $4, 1)
         ON CONFLICT (session_id, judge_model, rubric_version)
         DO UPDATE SET last_attempt_at = GREATEST(attempts.last_attempt_at,
                                                  EXCLUDED.last_attempt_at),
                       attempt_count = attempts.attempt_count + 1,
                       consecutive_failures = CASE
                           WHEN $5 THEN attempts.consecutive_failures + 1
                           ELSE 0
                       END,
                       window_started_at = CASE
                           WHEN attempts.window_started_at IS NULL
                             OR attempts.window_started_at < EXCLUDED.last_attempt_at - interval '24 hours'
                           THEN EXCLUDED.last_attempt_at
                           ELSE attempts.window_started_at
                       END,
                       window_attempts = CASE
                           WHEN attempts.window_started_at IS NULL
                             OR attempts.window_started_at < EXCLUDED.last_attempt_at - interval '24 hours'
                           THEN 1
                           ELSE attempts.window_attempts + 1
                       END
         RETURNING window_attempts",
        session_db_id,
        judge_model,
        rubric_version,
        attempted_at,
        failed,
    )
    .fetch_one(pool)
    .await?;
    Ok(window_attempts)
}

pub async fn fetch_session_quality(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<SessionQualityRow>, PersistError> {
    let row = sqlx::query_as!(
        SessionQualityRow,
        r#"SELECT outcome AS "outcome!",
                  quality_score::float8 AS quality_score,
                  rationale,
                  judge_model AS "judge_model!",
                  rubric_version AS "rubric_version!",
                  judged_at AS "judged_at!",
                  bundle_id,
                  verdict_id AS "verdict_id!",
                  end_turn_id,
                  end_label,
                  end_source,
                  end_rule,
                  end_confidence,
                  end_evidence,
                  failure,
                  COALESCE(failure_tags, '{}'::text[]) AS "failure_tags!",
                  severity
             FROM mike.session_quality_v
            WHERE session_id = $1"#,
        session_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_for_judgment_is_debug() {
        let row = SessionForJudgment {
            session_db_id: 1,
            session_id: "abc".to_string(),
            needs_verdict: true,
        };
        assert!(!format!("{row:?}").is_empty());
    }

    #[test]
    fn session_quality_row_is_debug() {
        let row = SessionQualityRow {
            outcome: "success".to_string(),
            quality_score: Some(0.85),
            rationale: Some("Good session.".to_string()),
            judge_model: "google/gemini-3.8-flash".to_string(),
            rubric_version: 1,
            judged_at: Utc::now(),
            bundle_id: Some(3),
            verdict_id: 9,
            end_turn_id: Some(4),
            end_label: Some("done".to_string()),
            end_source: Some("llm".to_string()),
            end_rule: None,
            end_confidence: Some("high".to_string()),
            end_evidence: None,
            failure: None,
            failure_tags: Vec::new(),
            severity: None,
        };
        assert!(!format!("{row:?}").is_empty());
    }
}
