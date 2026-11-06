use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct NewSessionEndItem<'a> {
    pub run_id: i64,
    pub end_turn_id: i64,
    pub session_id: i64,
    pub label_id: i64,
    pub verdict_id: Option<i64>,
    pub outcome: &'a str,
    pub label: &'a str,
    pub llm_end_label: Option<&'a str>,
    pub rule_fired: Option<&'a str>,
    pub effective_end_label: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SessionEndItemRow {
    pub run_id: i64,
    pub end_turn_id: i64,
    pub session_id: i64,
    pub label_id: i64,
    pub verdict_id: Option<i64>,
    pub outcome: String,
    pub label: String,
    pub llm_end_label: Option<String>,
    pub rule_fired: Option<String>,
    pub effective_end_label: Option<String>,
    pub llm_frustration_correct: Option<bool>,
    pub effective_correct: Option<bool>,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub total: i64,
}

fn frustration_correct(label: &str, answer: Option<&str>) -> Option<bool> {
    answer.map(|a| (a == "closed_in_frustration") == (label == "closed_in_frustration"))
}

impl PersistHandle {
    pub async fn insert_session_end_item(
        &self,
        i: &NewSessionEndItem<'_>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("insert_session_end_item", async move {
            let llm_frustration_correct = frustration_correct(i.label, i.llm_end_label);
            let effective_correct = i.effective_end_label.map(|e| e == i.label);
            sqlx::query!(
                "INSERT INTO eval.session_end_run_items
                    (run_id, end_turn_id, session_id, label_id, verdict_id, outcome, label,
                     llm_end_label, rule_fired, effective_end_label, llm_frustration_correct,
                     effective_correct)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT (run_id, end_turn_id) DO NOTHING",
                i.run_id,
                i.end_turn_id,
                i.session_id,
                i.label_id,
                i.verdict_id,
                i.outcome,
                i.label,
                i.llm_end_label,
                i.rule_fired,
                i.effective_end_label,
                llm_frustration_correct,
                effective_correct,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn list_session_end_items(
        &self,
        run_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<SessionEndItemRow>, i64), PersistError> {
        crate::metrics::time_db("list_session_end_items", async move {
            let rows = sqlx::query_as!(
                SessionEndItemRow,
                r#"SELECT run_id, end_turn_id, session_id, label_id, verdict_id, outcome, label,
                          llm_end_label, rule_fired, effective_end_label, llm_frustration_correct,
                          effective_correct, created_at, count(*) OVER () AS "total!"
                     FROM eval.session_end_run_items
                    WHERE run_id = $1
                    ORDER BY end_turn_id
                    LIMIT $2 OFFSET $3"#,
                run_id,
                limit,
                offset,
            )
            .fetch_all(&self.pool)
            .await?;
            let total = rows.first().map_or(0, |r| r.total);
            Ok((rows, total))
        })
        .await
    }
}
