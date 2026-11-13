use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

use super::PersistError;

pub struct ConsolidationActionRecord {
    pub lesson_id: i64,
    pub lesson_text: String,
    pub action_kind: &'static str,
    pub outcome: &'static str,
    pub error: Option<String>,
    pub before_stage: Option<&'static str>,
    pub after_stage: Option<&'static str>,
    pub decided_by: Option<&'static str>,
    pub distance: Option<f32>,
    pub threshold: Option<f32>,
    pub embedding_model: Option<String>,
    pub counterpart_lesson_id: Option<i64>,
    pub counterpart_uuid: Option<uuid::Uuid>,
    pub counterpart_text: Option<String>,
    pub runners_up: JsonValue,
    pub rank_detail: Option<JsonValue>,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn start_consolidation_run(
        self,
        kind: &str,
        trigger: &str,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("start_consolidation_run", async move {
            let id = sqlx::query_scalar!(
                r#"INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id)
                 SELECT k.id, t.id, s.id
                   FROM memory.consolidation_run_kinds k,
                        memory.consolidation_triggers t,
                        memory.consolidation_run_statuses s
                  WHERE k.code = $1 AND t.code = $2 AND s.code = 'running'
                 RETURNING id AS "id!""#,
                kind,
                trigger,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    #[rustfmt::skip]
    pub async fn finish_consolidation_run(self,
        run_id: i64,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("finish_consolidation_run", async move {
            sqlx::query!(
                "UPDATE memory.consolidation_runs r
                    SET status_id = s.id,
                        finished_at = NOW(),
                        error = $3
                   FROM memory.consolidation_run_statuses s
                  WHERE s.code = $2
                    AND r.id = $1",
                run_id,
                status,
                error,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn delete_consolidation_run(self, run_id: i64) -> Result<(), PersistError> {
        crate::metrics::time_db("delete_consolidation_run", async move {
            sqlx::query!(
                "DELETE FROM memory.consolidation_runs WHERE id = $1",
                run_id
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn nightly_ran_since(self, since: DateTime<Utc>) -> Result<bool, PersistError> {
        crate::metrics::time_db("nightly_ran_since", async move {
            let ran = sqlx::query_scalar!(
                r#"SELECT EXISTS (
                     SELECT 1
                       FROM memory.consolidation_runs r
                       JOIN memory.consolidation_run_kinds k
                         ON k.id = r.run_kind_id AND k.code = 'nightly'
                       JOIN memory.consolidation_run_statuses s
                         ON s.id = r.status_id AND s.code IN ('completed', 'partial')
                      WHERE r.started_at >= $1
                 ) AS "ran!""#,
                since,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(ran)
        })
        .await
    }

    pub async fn record_consolidation_actions(
        self,
        run_id: i64,
        records: &[ConsolidationActionRecord],
    ) -> Result<(), PersistError> {
        if records.is_empty() {
            return Ok(());
        }
        let cols = ConsolidationActionColumns::from_records(records);
        crate::metrics::time_db("record_consolidation_actions", async move {
            sqlx::query_file!(
                "src/store/persistence/consolidation_audit/record_actions.sql",
                run_id,
                &cols.lesson_ids,
                &cols.lesson_texts,
                &cols.action_kinds as &[&str],
                &cols.outcomes as &[&str],
                &cols.errors as &[Option<String>],
                &cols.before_stages as &[Option<&str>],
                &cols.after_stages as &[Option<&str>],
                &cols.decided_bys as &[Option<&str>],
                &cols.distances as &[Option<f32>],
                &cols.thresholds as &[Option<f32>],
                &cols.embedding_models as &[Option<String>],
                &cols.counterpart_ids as &[Option<i64>],
                &cols.counterpart_uuids as &[Option<uuid::Uuid>],
                &cols.counterpart_texts as &[Option<String>],
                &cols.runners_ups,
                &cols.rank_details as &[Option<JsonValue>],
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}

struct ConsolidationActionColumns {
    lesson_ids: Vec<i64>,
    lesson_texts: Vec<String>,
    action_kinds: Vec<&'static str>,
    outcomes: Vec<&'static str>,
    errors: Vec<Option<String>>,
    before_stages: Vec<Option<&'static str>>,
    after_stages: Vec<Option<&'static str>>,
    decided_bys: Vec<Option<&'static str>>,
    distances: Vec<Option<f32>>,
    thresholds: Vec<Option<f32>>,
    embedding_models: Vec<Option<String>>,
    counterpart_ids: Vec<Option<i64>>,
    counterpart_uuids: Vec<Option<uuid::Uuid>>,
    counterpart_texts: Vec<Option<String>>,
    runners_ups: Vec<JsonValue>,
    rank_details: Vec<Option<JsonValue>>,
}

impl ConsolidationActionColumns {
    fn from_records(records: &[ConsolidationActionRecord]) -> Self {
        let n = records.len();
        let mut cols = Self {
            lesson_ids: Vec::with_capacity(n),
            lesson_texts: Vec::with_capacity(n),
            action_kinds: Vec::with_capacity(n),
            outcomes: Vec::with_capacity(n),
            errors: Vec::with_capacity(n),
            before_stages: Vec::with_capacity(n),
            after_stages: Vec::with_capacity(n),
            decided_bys: Vec::with_capacity(n),
            distances: Vec::with_capacity(n),
            thresholds: Vec::with_capacity(n),
            embedding_models: Vec::with_capacity(n),
            counterpart_ids: Vec::with_capacity(n),
            counterpart_uuids: Vec::with_capacity(n),
            counterpart_texts: Vec::with_capacity(n),
            runners_ups: Vec::with_capacity(n),
            rank_details: Vec::with_capacity(n),
        };
        for r in records {
            cols.lesson_ids.push(r.lesson_id);
            cols.lesson_texts.push(r.lesson_text.clone());
            cols.action_kinds.push(r.action_kind);
            cols.outcomes.push(r.outcome);
            cols.errors.push(r.error.clone());
            cols.before_stages.push(r.before_stage);
            cols.after_stages.push(r.after_stage);
            cols.decided_bys.push(r.decided_by);
            cols.distances.push(r.distance);
            cols.thresholds.push(r.threshold);
            cols.embedding_models.push(r.embedding_model.clone());
            cols.counterpart_ids.push(r.counterpart_lesson_id);
            cols.counterpart_uuids.push(r.counterpart_uuid);
            cols.counterpart_texts.push(r.counterpart_text.clone());
            cols.runners_ups.push(r.runners_up.clone());
            cols.rank_details.push(r.rank_detail.clone());
        }
        cols
    }
}
