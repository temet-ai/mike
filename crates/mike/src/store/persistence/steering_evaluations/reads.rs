use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder};

use super::super::PersistError;
use super::{EvaluationListRow, EvaluationRow, EvaluationStatsRow};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvaluationFilter {
    pub session_uuid: uuid::Uuid,
    pub steerer_id: Option<i64>,
    pub question: Option<String>,
    pub mode: Option<String>,
    pub status: Option<String>,
    pub before_id: Option<i64>,
    pub limit: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvaluationStatsFilter {
    pub steerer_id: Option<i64>,
    pub question: Option<String>,
    pub mode: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

const LIST_SELECT: &str = "WITH filtered AS (
    SELECT e.id, e.uuid, e.steerer_id, st.name AS steerer_name, st.kind AS steerer_kind,
           e.operator_turn_id, e.status, e.mode, e.question, e.trigger_point,
           e.trigger_request_id, e.bundle_id, e.intervention_uuid, e.dispatched_at,
           e.resolved_at, e.verdict_json, e.confidence, e.reason, e.outcome_label,
           e.label_source, e.labelled_at, e.label_evidence,
           count(*) OVER () AS total_count
      FROM steering.evaluations e
      JOIN steering.steerers st ON st.id = e.steerer_id
     WHERE e.session_id = ";

const LIST_TAIL: &str = ")
SELECT f.id, f.uuid, f.steerer_id, f.steerer_name, f.steerer_kind, f.operator_turn_id, f.status,
       f.mode, f.question, f.trigger_point, f.trigger_request_id, f.bundle_id,
       f.intervention_uuid, f.dispatched_at, f.resolved_at, f.verdict_json, f.confidence,
       f.reason, f.outcome_label, f.label_source, f.labelled_at, f.label_evidence, f.total_count
  FROM filtered f
 WHERE f.id < ";

fn page_query(
    session_db_id: i64,
    f: &EvaluationFilter,
    before_id: Option<i64>,
    limit: i64,
) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(LIST_SELECT);
    qb.push_bind(session_db_id);
    if let Some(steerer_id) = f.steerer_id {
        qb.push(" AND e.steerer_id = ").push_bind(steerer_id);
    }
    if let Some(question) = &f.question {
        qb.push(" AND e.question = ").push_bind(question);
    }
    if let Some(mode) = &f.mode {
        qb.push(" AND e.mode = ").push_bind(mode);
    }
    if let Some(status) = &f.status {
        qb.push(" AND e.status = ").push_bind(status);
    }
    qb.push(LIST_TAIL).push_bind(before_id.unwrap_or(i64::MAX));
    qb.push(" ORDER BY f.id DESC LIMIT ").push_bind(limit);
    qb
}

const STATS_HEAD: &str = "WITH scored AS (
    SELECT e.steerer_id, st.name AS steerer_name, e.question, e.mode, e.status, e.outcome_label,
           (e.status = 'resolved_correction'
            AND (r.claude_code_agent_id IS NOT NULL OR r.agent_instance_id IS NOT NULL
                 OR r.subagent_persona IS NOT NULL)
            AND i.addressed_to IS DISTINCT FROM 'parent_for_actor') AS non_actor
      FROM steering.evaluations e
      JOIN steering.steerers st ON st.id = e.steerer_id
      LEFT JOIN requests.requests r ON r.id = e.trigger_request_id
      LEFT JOIN steering.interventions i ON i.uuid = e.intervention_uuid
     WHERE e.dispatched_at >= ";

const STATS_TAIL: &str = ")
SELECT steerer_id, steerer_name, question, mode,
       count(*) AS evaluations,
       count(*) FILTER (WHERE status = 'dispatched') AS in_flight,
       count(*) FILTER (WHERE status = 'resolved_correction') AS fires,
       count(*) FILTER (WHERE status = 'resolved_pass') AS passes,
       count(*) FILTER (WHERE status = 'resolved_correction' AND NOT non_actor
                        AND outcome_label = 'right') AS tp,
       count(*) FILTER (WHERE status = 'resolved_correction'
                        AND (non_actor OR outcome_label = 'wrong')) AS fp,
       count(*) FILTER (WHERE status = 'resolved_pass' AND outcome_label = 'right') AS tn,
       count(*) FILTER (WHERE status = 'resolved_pass' AND outcome_label = 'wrong') AS fn,
       count(*) FILTER (WHERE outcome_label = 'indeterminate' AND NOT non_actor) AS indeterminate,
       count(*) FILTER (WHERE status <> 'dispatched' AND outcome_label IS NULL
                        AND NOT non_actor) AS unlabelled,
       count(*) FILTER (WHERE non_actor) AS non_actor_fires
  FROM scored
 GROUP BY steerer_id, steerer_name, question, mode
 ORDER BY steerer_id, question NULLS FIRST, mode";

fn stats_query(f: &EvaluationStatsFilter, since: DateTime<Utc>) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(STATS_HEAD);
    qb.push_bind(since);
    if let Some(steerer_id) = f.steerer_id {
        qb.push(" AND e.steerer_id = ").push_bind(steerer_id);
    }
    if let Some(question) = &f.question {
        qb.push(" AND e.question = ").push_bind(question);
    }
    if let Some(mode) = &f.mode {
        qb.push(" AND e.mode = ").push_bind(mode);
    }
    if let Some(until) = f.until {
        qb.push(" AND e.dispatched_at < ").push_bind(until);
    }
    qb.push(STATS_TAIL);
    qb
}

#[derive(sqlx::FromRow)]
struct CountedRow {
    #[sqlx(flatten)]
    row: EvaluationListRow,
    total_count: i64,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn get_latest_evaluation_for_session(
        self,
        session_uuid: uuid::Uuid,
        steerer_kind: &str,
    ) -> Result<Option<EvaluationRow>, PersistError> {
        crate::metrics::time_db("get_latest_evaluation_for_session", async move {
            let Some(session_db_id) = self.session_row_id(session_uuid).await? else {
                return Ok(None);
            };
            let row = sqlx::query_as!(
                EvaluationRow,
                "SELECT e.uuid, e.operator_turn_id, e.status, e.intervention_uuid,
                        e.dispatched_at, e.resolved_at
                   FROM steering.evaluations e
                   JOIN steering.steerers st ON st.id = e.steerer_id
                  WHERE e.session_id = $1
                    AND e.mode = 'live'
                    AND st.kind = $2
                  ORDER BY e.dispatched_at DESC
                  LIMIT 1",
                session_db_id,
                steerer_kind,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    async fn evaluation_page(
        self,
        f: &EvaluationFilter,
        before_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<CountedRow>, PersistError> {
        let Some(session_db_id) = self.session_row_id(f.session_uuid).await? else {
            return Ok(Vec::new());
        };
        Ok(page_query(session_db_id, f, before_id, limit)
            .build_query_as::<CountedRow>()
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn list_session_evaluations(
        self,
        f: &EvaluationFilter,
    ) -> Result<(Vec<EvaluationListRow>, i64), PersistError> {
        crate::metrics::time_db("list_session_evaluations", async move {
            let rows = self.evaluation_page(f, f.before_id, f.limit).await?;
            let total = match rows.first() {
                Some(r) => r.total_count,
                None if f.before_id.is_some() => self
                    .evaluation_page(f, None, 1)
                    .await?
                    .first()
                    .map_or(0, |r| r.total_count),
                None => 0,
            };
            Ok((rows.into_iter().map(|r| r.row).collect(), total))
        })
        .await
    }

    pub async fn count_session_evaluations(
        self,
        f: &EvaluationFilter,
    ) -> Result<i64, PersistError> {
        let probe = EvaluationFilter {
            before_id: None,
            limit: 1,
            ..f.clone()
        };
        Ok(self.list_session_evaluations(&probe).await?.1)
    }

    pub async fn evaluation_stats(
        self,
        f: &EvaluationStatsFilter,
    ) -> Result<Vec<EvaluationStatsRow>, PersistError> {
        let since = f
            .since
            .unwrap_or_else(|| Utc::now() - chrono::Duration::days(14));
        crate::metrics::time_db("evaluation_stats", async move {
            let rows = stats_query(f, since)
                .build_query_as::<EvaluationStatsRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }
}
