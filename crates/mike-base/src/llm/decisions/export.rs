use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::store::persistence::PersistError;

pub const EXPORT_FORMAT: &str = "mike-decisions-v1";

const EXPORT_SQL: &str = "SELECT jsonb_build_object('format', $3::text)
        || to_jsonb(r)
        || jsonb_build_object('outcomes', COALESCE(o.outcomes, '[]'::jsonb))
   FROM decisions.records r
   LEFT JOIN LATERAL (
       SELECT jsonb_agg(jsonb_build_object('label', x.label, 'label_source', x.label_source,
                                           'evidence', x.evidence, 'labelled_at', x.labelled_at)
                        ORDER BY x.labelled_at) AS outcomes
         FROM decisions.outcomes x
        WHERE x.record_id = r.id AND $4
   ) o ON true
  WHERE r.created_at >= $1 AND ($2::text IS NULL OR r.point = $2)
  ORDER BY r.id";

impl super::DecisionStore<'_> {
    pub async fn export_decisions(
        &self,
        since: DateTime<Utc>,
        point: Option<&str>,
        with_outcomes: bool,
    ) -> Result<Vec<Value>, PersistError> {
        crate::metrics::time_db("export_decisions", async move {
            Ok(sqlx::query_scalar::<_, Value>(EXPORT_SQL)
                .bind(since)
                .bind(point)
                .bind(EXPORT_FORMAT)
                .bind(with_outcomes)
                .fetch_all(self.0.pool())
                .await?)
        })
        .await
    }

    pub async fn decision_source_refs(&self, prefix: &str) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("decision_source_refs", async move {
            Ok(sqlx::query_scalar::<_, String>(
                "SELECT source_ref FROM decisions.records WHERE source_ref LIKE $1 || '%'",
            )
            .bind(prefix)
            .fetch_all(self.0.pool())
            .await?)
        })
        .await
    }
}
