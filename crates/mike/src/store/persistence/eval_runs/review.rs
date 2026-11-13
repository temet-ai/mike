use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;
use super::super::eval_labels::EvalLabelRow;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalReviewContext {
    pub operator_turn_id: i64,
    pub session_uuid: uuid::Uuid,
    pub closed_at: Option<DateTime<Utc>>,
    pub user_prompt: Option<String>,
    pub final_assistant_text: Option<String>,
    pub facts_present: bool,
    pub text_chars: Option<i32>,
    pub text_cut_at: Option<i32>,
    pub tool_call_count: Option<i64>,
    pub tool_error_count: Option<i64>,
    pub next_state: Option<String>,
    pub next_genuine_turn_id: Option<i64>,
    pub next_genuine_request_id: Option<i64>,
    pub next_genuine_at: Option<DateTime<Utc>>,
    pub next_genuine_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvalReviewLabels {
    pub run_gold: Vec<EvalLabelRow>,
    pub current: Vec<EvalLabelRow>,
    pub reviews: Vec<EvalLabelRow>,
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn eval_run_has_item(self, run_id: i64, turn_id: i64) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM eval.run_items
                               WHERE run_id = $1 AND operator_turn_id = $2) AS "has_item!""#,
            run_id,
            turn_id,
        )
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn eval_review_context(
        self,
        turn_id: i64,
    ) -> Result<Option<EvalReviewContext>, PersistError> {
        crate::metrics::time_db("eval_review_context", async move {
            Ok(sqlx::query_as!(
                EvalReviewContext,
                r#"SELECT t.id AS "operator_turn_id!", s.session_uuid AS "session_uuid!", t.closed_at,
                          t.user_prompt, t.final_assistant_text,
                          (f.operator_turn_id IS NOT NULL) AS "facts_present!",
                          f.text_chars AS "text_chars?", f.text_cut_at AS "text_cut_at?",
                          t.tool_call_count, t.tool_error_count, f.next_state AS "next_state?",
                          f.next_genuine_turn_id, f.next_genuine_request_id, f.next_genuine_at,
                          COALESCE(nt.user_prompt, nr.human_prompt) AS "next_genuine_text?"
                     FROM memory.operator_turns t
                     JOIN sessions.sessions s ON s.id = t.session_id
                     LEFT JOIN memory.turn_facts f ON f.operator_turn_id = t.id
                     LEFT JOIN memory.operator_turns nt ON nt.id = f.next_genuine_turn_id
                     LEFT JOIN requests.requests nr ON nr.id = f.next_genuine_request_id
                    WHERE t.id = $1"#,
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn eval_review_labels(
        self,
        run_id: i64,
        turn_id: i64,
    ) -> Result<EvalReviewLabels, PersistError> {
        crate::metrics::time_db("eval_review_labels", async move {
            let run_gold = sqlx::query_file_as!(
                EvalLabelRow,
                "src/store/persistence/eval_runs/review_run_gold.sql",
                run_id,
                turn_id,
            )
            .fetch_all(&self.pool)
            .await?;
            let current = sqlx::query_file_as!(
                EvalLabelRow,
                "src/store/persistence/eval_runs/review_current.sql",
                turn_id,
            )
            .fetch_all(&self.pool)
            .await?;
            let reviews = sqlx::query_file_as!(
                EvalLabelRow,
                "src/store/persistence/eval_runs/review_history.sql",
                turn_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(EvalReviewLabels {
                run_gold,
                current,
                reviews,
            })
        })
        .await
    }
}
