use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[cfg(test)]
pub mod tests;

#[derive(Debug, Clone, PartialEq)]
pub struct SuppliedLesson {
    pub lesson_id: i64,
    pub route: &'static str,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarriedCard {
    pub hash: String,
    pub added_by_proxy: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SuppliedItemRow {
    pub id: i64,
    pub request_id: i64,
    pub session_id: Uuid,
    pub item_kind: String,
    pub lesson_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub thread_slug: Option<String>,
    pub card_hash: Option<String>,
    pub route: String,
    pub score: Option<f64>,
    pub chance: Option<f64>,
    pub item_text: String,
    pub supplied_at: DateTime<Utc>,
    pub verdict: Option<String>,
    pub rationale: Option<String>,
    pub evidence: Option<String>,
    pub judged_at: Option<DateTime<Utc>>,
    pub hand_verdict: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingVerdict {
    pub item_id: i64,
    pub item_kind: String,
    pub route: String,
    pub item_text: String,
    pub session_id: i64,
    pub turn_id: i64,
    pub user_prompt: Option<String>,
    pub final_assistant_text: Option<String>,
    pub tools_used: Vec<String>,
    pub files_touched: Vec<String>,
    pub reaction: Option<String>,
    pub next_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewVerdict<'a> {
    pub item_id: i64,
    pub verdict: &'a str,
    pub rationale: &'a str,
    pub evidence: &'a str,
    pub model: &'a str,
    pub input_text: &'a str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplyFigure {
    pub item_kind: String,
    pub route: String,
    pub supplied: i64,
    pub bytes: i64,
    pub judged: i64,
    pub used: i64,
    pub corrected_anyway: i64,
    pub ignored: i64,
    pub bytes_ignored: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HandCheckRow {
    pub supplied_item_id: i64,
    pub request_id: i64,
    pub item_kind: String,
    pub route: String,
    pub item_text: String,
    pub verdict: String,
    pub rationale: String,
    pub evidence: String,
    pub input_text: String,
    pub judged_at: DateTime<Utc>,
    pub hand_verdict: Option<String>,
    pub hand_checked_by: Option<String>,
    pub hand_checked_at: Option<DateTime<Utc>>,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn record_supplied_lessons(
        self,
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

    pub async fn record_supplied_cards(
        self,
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

    pub async fn supplied_items_for_request(
        self,
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

    pub async fn supplied_items_for_session(
        self,
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

    pub async fn supplied_items_pending_verdict(
        self,
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

    pub async fn insert_supply_verdict(
        self,
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

    pub async fn supply_figures(
        self,
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

    pub async fn supply_hand_sample(
        self,
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

    pub async fn set_supply_hand_verdict(
        self,
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
