use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

#[cfg(test)]
pub(crate) mod tests;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SuppliedLesson {
    pub(crate) lesson_id: i64,
    pub(crate) route: &'static str,
    pub(crate) score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedCard {
    pub(crate) hash: String,
    pub(crate) added_by_proxy: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct SuppliedItemRow {
    pub(crate) id: i64,
    pub(crate) request_id: i64,
    pub(crate) session_id: Uuid,
    pub(crate) item_kind: String,
    pub(crate) lesson_id: Option<i64>,
    pub(crate) thread_id: Option<i64>,
    pub(crate) thread_slug: Option<String>,
    pub(crate) card_hash: Option<String>,
    pub(crate) route: String,
    pub(crate) score: Option<f64>,
    pub(crate) chance: Option<f64>,
    pub(crate) item_text: String,
    pub(crate) supplied_at: DateTime<Utc>,
    pub(crate) verdict: Option<String>,
    pub(crate) rationale: Option<String>,
    pub(crate) evidence: Option<String>,
    pub(crate) judged_at: Option<DateTime<Utc>>,
    pub(crate) hand_verdict: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingVerdict {
    pub(crate) item_id: i64,
    pub(crate) item_kind: String,
    pub(crate) route: String,
    pub(crate) item_text: String,
    pub(crate) session_id: i64,
    pub(crate) turn_id: i64,
    pub(crate) user_prompt: Option<String>,
    pub(crate) final_assistant_text: Option<String>,
    pub(crate) tools_used: Vec<String>,
    pub(crate) files_touched: Vec<String>,
    pub(crate) reaction: Option<String>,
    pub(crate) next_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewVerdict<'a> {
    pub(crate) item_id: i64,
    pub(crate) verdict: &'a str,
    pub(crate) rationale: &'a str,
    pub(crate) evidence: &'a str,
    pub(crate) model: &'a str,
    pub(crate) input_text: &'a str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct SupplyFigure {
    pub(crate) item_kind: String,
    pub(crate) route: String,
    pub(crate) supplied: i64,
    pub(crate) bytes: i64,
    pub(crate) judged: i64,
    pub(crate) used: i64,
    pub(crate) corrected_anyway: i64,
    pub(crate) ignored: i64,
    pub(crate) bytes_ignored: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct HandCheckRow {
    pub(crate) supplied_item_id: i64,
    pub(crate) request_id: i64,
    pub(crate) item_kind: String,
    pub(crate) route: String,
    pub(crate) item_text: String,
    pub(crate) verdict: String,
    pub(crate) rationale: String,
    pub(crate) evidence: String,
    pub(crate) input_text: String,
    pub(crate) judged_at: DateTime<Utc>,
    pub(crate) hand_verdict: Option<String>,
    pub(crate) hand_checked_by: Option<String>,
    pub(crate) hand_checked_at: Option<DateTime<Utc>>,
}

impl PersistHandle {
    pub(crate) async fn record_supplied_lessons(
        &self,
        request_id: i64,
        session: Uuid,
        lessons: &[SuppliedLesson],
    ) -> Result<u64, PersistError> {
        if lessons.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = lessons.iter().map(|l| l.lesson_id).collect();
        let routes: Vec<String> = lessons.iter().map(|l| l.route.to_owned()).collect();
        let scores: Vec<Option<f64>> = lessons.iter().map(|l| l.score).collect();
        crate::metrics::time_db("record_supplied_lessons", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/supplied_items/record_lessons.sql",
                request_id,
                &ids,
                &routes,
                &scores as &[Option<f64>],
                session,
            )
            .execute(self.pool())
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub(crate) async fn record_supplied_cards(
        &self,
        request_id: i64,
        session: Uuid,
        cards: &[CarriedCard],
    ) -> Result<u64, PersistError> {
        if cards.is_empty() {
            return Ok(0);
        }
        let hashes: Vec<String> = cards.iter().map(|c| c.hash.clone()).collect();
        let proxy: Vec<bool> = cards.iter().map(|c| c.added_by_proxy).collect();
        crate::metrics::time_db("record_supplied_cards", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/supplied_items/record_cards.sql",
                request_id,
                &hashes,
                &proxy,
                session,
            )
            .execute(self.pool())
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub(crate) async fn supplied_items_for_request(
        &self,
        request_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SuppliedItemRow>, PersistError> {
        let rows = sqlx::query_file_as!(
            SuppliedItemRow,
            "src/store/persistence/supplied_items/list_by_request.sql",
            request_id,
            limit,
            offset
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    pub(crate) async fn supplied_items_for_session(
        &self,
        session: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SuppliedItemRow>, PersistError> {
        let rows = sqlx::query_file_as!(
            SuppliedItemRow,
            "src/store/persistence/supplied_items/list_by_session.sql",
            session,
            limit,
            offset
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    pub(crate) async fn supplied_items_pending_verdict(
        &self,
        limit: i64,
        supplied_since: DateTime<Utc>,
        closed_before: DateTime<Utc>,
    ) -> Result<Vec<PendingVerdict>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/supplied_items/pending_verdicts.sql",
            limit,
            supplied_since,
            closed_before
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| PendingVerdict {
                item_id: r.id,
                item_kind: r.item_kind,
                route: r.route,
                item_text: r.item_text,
                session_id: r.session_id,
                turn_id: r.turn_id,
                user_prompt: r.user_prompt,
                final_assistant_text: r.final_assistant_text,
                tools_used: r.tools_used.unwrap_or_default(),
                files_touched: r.files_touched.unwrap_or_default(),
                reaction: r.reaction,
                next_message: r.next_message,
            })
            .collect())
    }

    pub(crate) async fn insert_supply_verdict(
        &self,
        verdict: &NewVerdict<'_>,
    ) -> Result<bool, PersistError> {
        let done = sqlx::query_file!(
            "src/store/persistence/supplied_items/insert_verdict.sql",
            verdict.item_id,
            verdict.verdict,
            verdict.rationale,
            verdict.evidence,
            verdict.model,
            verdict.input_text
        )
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub(crate) async fn supply_figures(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<SupplyFigure>, PersistError> {
        let rows = sqlx::query_file_as!(
            SupplyFigure,
            "src/store/persistence/supplied_items/figures.sql",
            since
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    pub(crate) async fn supply_hand_sample(
        &self,
        week_start: DateTime<Utc>,
        size: i64,
    ) -> Result<Vec<HandCheckRow>, PersistError> {
        let rows = sqlx::query_file_as!(
            HandCheckRow,
            "src/store/persistence/supplied_items/hand_sample.sql",
            week_start,
            size
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    pub(crate) async fn set_supply_hand_verdict(
        &self,
        item_id: i64,
        verdict: &str,
        checked_by: &str,
    ) -> Result<bool, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/supplied_items/set_hand_verdict.sql",
            item_id,
            verdict,
            checked_by
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.is_some())
    }
}
