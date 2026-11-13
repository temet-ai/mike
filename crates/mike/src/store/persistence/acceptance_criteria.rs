use chrono::{DateTime, Utc};

use super::PersistError;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AcceptanceCriterionRow {
    pub ord: i32,
    pub criterion: String,
    pub provenance: String,
    pub operator_span: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceCriteriaMap {
    pub id: i64,
    pub version: i32,
    pub ask_entry_count: i32,
    pub model: Option<String>,
    pub output_tokens: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub criteria: Vec<AcceptanceCriterionRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCriterion {
    pub criterion: String,
    pub provenance: String,
    pub operator_span: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AskSeen {
    pub ask_entry_count: i32,
    pub newest_ask_entry_id: Option<i64>,
}

impl AskSeen {
    pub fn count_only(ask_entry_count: i32) -> Self {
        Self {
            ask_entry_count,
            newest_ask_entry_id: None,
        }
    }
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn insert_acceptance_criteria_map(
        self,
        session_db_id: i64,
        seen: AskSeen,
        model: Option<&str>,
        output_tokens: Option<i32>,
        criteria: &[NewCriterion],
    ) -> Result<i64, PersistError> {
        let mut tx = self.pool.begin().await?;

        let next_version: i32 = sqlx::query_scalar!(
            r#"SELECT COALESCE(MAX(version), 0) + 1 AS "next!"
                 FROM memory.acceptance_criteria_maps
                WHERE session_id = $1"#,
            session_db_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        let map_id: i64 = sqlx::query_scalar!(
            r#"INSERT INTO memory.acceptance_criteria_maps
                   (session_id, version, ask_entry_count, last_ask_entry_id, model, output_tokens)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id"#,
            session_db_id,
            next_version,
            seen.ask_entry_count,
            seen.newest_ask_entry_id,
            model,
            output_tokens,
        )
        .fetch_one(&mut *tx)
        .await?;

        if !criteria.is_empty() {
            let ords: Vec<i32> = (0..criteria.len() as i32).collect();
            let texts: Vec<String> = criteria.iter().map(|c| c.criterion.clone()).collect();
            let provenances: Vec<String> = criteria.iter().map(|c| c.provenance.clone()).collect();
            let spans: Vec<Option<String>> =
                criteria.iter().map(|c| c.operator_span.clone()).collect();
            sqlx::query!(
                "INSERT INTO memory.acceptance_criteria
                     (map_id, ord, criterion, provenance, operator_span)
                 SELECT $1, UNNEST($2::int[]), UNNEST($3::text[]), UNNEST($4::text[]),
                        UNNEST($5::text[])",
                map_id,
                &ords as &[i32],
                &texts as &[String],
                &provenances as &[String],
                &spans as &[Option<String>],
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(map_id)
    }

    pub async fn fetch_session_project_path(
        self,
        session_db_id: i64,
    ) -> Result<Option<String>, PersistError> {
        let path = sqlx::query_scalar!(
            r#"SELECT p.project_path
                 FROM sessions.sessions s
                 JOIN projects.projects p ON p.id = s.project_id
                WHERE s.id = $1"#,
            session_db_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(path)
    }

    pub async fn fetch_acceptance_criteria_ask_seen(
        self,
        session_db_id: i64,
    ) -> Result<Option<AskSeen>, PersistError> {
        let row = sqlx::query!(
            r#"SELECT ask_entry_count, last_ask_entry_id
                 FROM memory.acceptance_criteria_maps
                WHERE session_id = $1
                ORDER BY version DESC
                LIMIT 1"#,
            session_db_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| AskSeen {
            ask_entry_count: r.ask_entry_count,
            newest_ask_entry_id: r.last_ask_entry_id,
        }))
    }

    pub async fn fetch_current_acceptance_criteria(
        self,
        session_db_id: i64,
    ) -> Result<Option<AcceptanceCriteriaMap>, PersistError> {
        let map = sqlx::query!(
            r#"SELECT id, version, ask_entry_count, model, output_tokens, created_at
                 FROM memory.acceptance_criteria_maps
                WHERE session_id = $1
                ORDER BY version DESC
                LIMIT 1"#,
            session_db_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(map) = map else {
            return Ok(None);
        };

        let criteria = sqlx::query_as!(
            AcceptanceCriterionRow,
            r#"SELECT ord, criterion, provenance, operator_span
                 FROM memory.acceptance_criteria
                WHERE map_id = $1
                ORDER BY ord ASC"#,
            map.id,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(AcceptanceCriteriaMap {
            id: map.id,
            version: map.version,
            ask_entry_count: map.ask_entry_count,
            model: map.model,
            output_tokens: map.output_tokens,
            created_at: map.created_at,
            criteria,
        }))
    }

    pub async fn fetch_acceptance_criteria_history(
        self,
        session_db_id: i64,
    ) -> Result<Vec<AcceptanceCriteriaMap>, PersistError> {
        let maps = sqlx::query!(
            r#"SELECT id, version, ask_entry_count, model, output_tokens, created_at
                 FROM memory.acceptance_criteria_maps
                WHERE session_id = $1
                ORDER BY version DESC"#,
            session_db_id,
        )
        .fetch_all(&self.pool)
        .await?;
        if maps.is_empty() {
            return Ok(Vec::new());
        }

        let map_ids: Vec<i64> = maps.iter().map(|m| m.id).collect();
        let rows = sqlx::query!(
            r#"SELECT map_id, ord, criterion, provenance, operator_span
                 FROM memory.acceptance_criteria
                WHERE map_id = ANY($1)
                ORDER BY map_id, ord ASC"#,
            &map_ids,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(maps
            .into_iter()
            .map(|m| AcceptanceCriteriaMap {
                id: m.id,
                version: m.version,
                ask_entry_count: m.ask_entry_count,
                model: m.model,
                output_tokens: m.output_tokens,
                created_at: m.created_at,
                criteria: rows
                    .iter()
                    .filter(|r| r.map_id == m.id)
                    .map(|r| AcceptanceCriterionRow {
                        ord: r.ord,
                        criterion: r.criterion.clone(),
                        provenance: r.provenance.clone(),
                        operator_span: r.operator_span.clone(),
                    })
                    .collect(),
            })
            .collect())
    }
}
