use super::statements::{CreatedStatement, NewStatements};
use crate::store::persistence::PersistError;
use chrono::{DateTime, Utc};
use serde_json::Value;

const DETACHED_GRACE_SECS: f64 = 1800.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassageRange {
    pub start: i32,
    pub end: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rehomed {
    pub rehomed: Vec<(i64, PassageRange)>,
    pub gone: u64,
    pub uncovered: Vec<PassageRange>,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn rehome_document_statements(
        self,
        item_id: i64,
        body: &str,
        judge_after: DateTime<Utc>,
    ) -> Result<Rehomed, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/rehome.sql",
            item_id,
            body,
            judge_after
        )
        .fetch_all(self.pool())
        .await?;
        let mut out = Rehomed::default();
        for row in rows {
            let range = match (row.range_start, row.range_end) {
                (Some(start), Some(end)) => Some(PassageRange { start, end }),
                _ => None,
            };
            match (row.outcome.as_str(), row.statement_id, range) {
                ("rehomed", Some(id), Some(range)) => out.rehomed.push((id, range)),
                ("gone", ..) => out.gone += 1,
                ("uncovered", _, Some(range)) => out.uncovered.push(range),
                _ => {}
            }
        }
        out.rehomed.sort_by_key(|(_, range)| range.start);
        out.uncovered.sort_by_key(|range| range.start);
        Ok(out)
    }

    pub async fn add_item_statements(
        self,
        item_id: i64,
        new: &NewStatements,
    ) -> Result<Vec<CreatedStatement>, PersistError> {
        let payload: Value = serde_json::to_value(&new.statements)?;
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/add_statements.sql",
            item_id,
            &new.split_hash,
            &new.model_key,
            new.stated_at,
            payload
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| CreatedStatement {
                id: row.id,
                ordinal: row.ordinal,
            })
            .collect())
    }

    pub async fn sweep_detached_statements(
        self,
        judge_after: DateTime<Utc>,
    ) -> Result<u64, PersistError> {
        let swept = sqlx::query_scalar!(
            r#"SELECT memory.sweep_detached_knowledge_statements(make_interval(secs => $1::float8), $2::timestamptz) AS "swept!""#,
            DETACHED_GRACE_SECS,
            judge_after
        )
        .fetch_one(self.pool())
        .await?;
        Ok(u64::try_from(swept).unwrap_or(0))
    }
}
