use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EndLabelRow {
    pub end_turn_id: i64,
    pub session_id: i64,
    pub label: String,
    pub source: String,
    pub rule: Option<String>,
    pub confidence: Option<String>,
    pub failure_tags: Vec<String>,
    pub severity: Option<String>,
    pub verdict_id: Option<i64>,
    pub label_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SessionVerdictFull {
    pub id: i64,
    pub session_id: i64,
    pub outcome: String,
    pub quality_score: Option<f64>,
    pub rationale: Option<String>,
    pub judge_model: String,
    pub rubric_version: i32,
    pub judged_at: DateTime<Utc>,
    pub mode: String,
    pub bundle_id: Option<i64>,
    pub rubric_id: Option<String>,
    pub end_turn_id: Option<i64>,
    pub end_label: Option<String>,
    pub end_source: Option<String>,
    pub end_rule: Option<String>,
    pub end_confidence: Option<String>,
    pub end_evidence: Option<String>,
    pub failure: Option<String>,
    pub failure_tags: Vec<String>,
    pub severity: Option<String>,
    pub slices: Vec<String>,
    pub as_of: Option<DateTime<Utc>>,
    pub digest_version: Option<String>,
    pub input_text: Option<String>,
    pub llm_output: Option<serde_json::Value>,
    pub generation_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
    #[serde(skip)]
    pub total: i64,
}

impl PersistHandle {
    pub async fn session_end_labels_for_turns(
        &self,
        turn_ids: &[i64],
        as_of: DateTime<Utc>,
    ) -> Result<Vec<EndLabelRow>, PersistError> {
        if turn_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("session_end_labels_for_turns", async move {
            Ok(sqlx::query_file_as!(
                EndLabelRow,
                "src/store/persistence/session_end/end_labels.sql",
                turn_ids,
                as_of,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn session_verdict_history(
        &self,
        session_db_id: i64,
        mode: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<SessionVerdictFull>, i64), PersistError> {
        crate::metrics::time_db("session_verdict_history", async move {
            let rows = match mode {
                Some(mode) => {
                    sqlx::query_file_as!(
                        SessionVerdictFull,
                        "src/store/persistence/session_end/history_by_mode.sql",
                        session_db_id,
                        mode,
                        limit,
                        offset,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        SessionVerdictFull,
                        "src/store/persistence/session_end/history_all.sql",
                        session_db_id,
                        limit,
                        offset,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };
            let total = rows.first().map_or(0, |r| r.total);
            Ok((rows, total))
        })
        .await
    }

    pub async fn session_end_for_session(
        &self,
        session_db_id: i64,
    ) -> Result<Option<EndLabelRow>, PersistError> {
        let end = crate::metrics::time_db("session_end_for_session", async move {
            let turn = sqlx::query_scalar!(
                r#"SELECT v.end_turn_id AS "end_turn_id!"
                     FROM session_judges.verdicts v
                    WHERE v.session_id = $1 AND v.mode = 'live' AND v.end_turn_id IS NOT NULL
                    ORDER BY v.judged_at DESC, v.id DESC
                    LIMIT 1"#,
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok::<_, PersistError>(turn)
        })
        .await?;
        let Some(turn) = end else {
            return Ok(None);
        };
        Ok(self
            .session_end_labels_for_turns(&[turn], Utc::now())
            .await?
            .into_iter()
            .next())
    }
}
