use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LessonEvidenceRow {
    pub id: i64,
    pub verdict_id: i64,
    pub operator_turn_id: i64,
    pub reason: String,
    pub similar: serde_json::Value,
    pub model: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LessonInjectionSummary {
    pub count: i64,
    pub first_injected_at: Option<DateTime<Utc>>,
    pub last_injected_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct EvidenceOrigin {
    operator_turn_id: i64,
    session_uuid: uuid::Uuid,
    session_title: Option<String>,
    turn_closed_at: Option<DateTime<Utc>>,
}

fn origin_turn_ids(rows: &[LessonEvidenceRow]) -> Vec<i64> {
    let mut ids: Vec<i64> = rows
        .iter()
        .filter_map(|r| r.similar.as_array())
        .flatten()
        .filter_map(|entry| entry.get("origin_turn_id").and_then(Value::as_i64))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn annotate_similar(similar: &mut Value, origins: &HashMap<i64, EvidenceOrigin>) {
    let Some(entries) = similar.as_array_mut() else {
        return;
    };
    for entry in entries {
        let Some(turn_id) = entry.get("origin_turn_id").and_then(Value::as_i64) else {
            continue;
        };
        let Some(origin) = origins.get(&turn_id) else {
            continue;
        };
        let Some(map) = entry.as_object_mut() else {
            continue;
        };
        map.insert(
            "origin_session_uuid".to_owned(),
            serde_json::json!(origin.session_uuid),
        );
        map.insert(
            "origin_session_title".to_owned(),
            serde_json::json!(origin.session_title),
        );
        map.insert(
            "origin_turn_closed_at".to_owned(),
            serde_json::json!(origin.turn_closed_at),
        );
    }
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    async fn evidence_origins(
        self,
        turn_ids: &[i64],
    ) -> Result<HashMap<i64, EvidenceOrigin>, PersistError> {
        if turn_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as!(
            EvidenceOrigin,
            r#"SELECT ot.id AS "operator_turn_id!", s.session_uuid AS "session_uuid!",
                      s.title AS session_title, ot.closed_at AS turn_closed_at
                 FROM memory.operator_turns ot
                 JOIN sessions.sessions s ON s.id = ot.session_id
                WHERE ot.id = ANY($1)"#,
            turn_ids,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|o| (o.operator_turn_id, o)).collect())
    }

    pub async fn list_lesson_evidence(
        self,
        lesson_id: i64,
    ) -> Result<Vec<LessonEvidenceRow>, PersistError> {
        crate::metrics::time_db("list_lesson_evidence", async move {
            let mut rows = sqlx::query_as!(
                LessonEvidenceRow,
                r#"SELECT id, verdict_id, operator_turn_id, reason,
                          similar_corrections AS "similar!", model, created_at
                     FROM memory.lesson_proposal_evidence
                    WHERE lesson_id = $1
                    ORDER BY created_at DESC"#,
                lesson_id,
            )
            .fetch_all(&self.pool)
            .await?;
            let origins = self.evidence_origins(&origin_turn_ids(&rows)).await?;
            for row in &mut rows {
                annotate_similar(&mut row.similar, &origins);
            }
            Ok(rows)
        })
        .await
    }

    pub async fn lesson_injection_stats(
        self,
        lesson_id: i64,
    ) -> Result<LessonInjectionSummary, PersistError> {
        crate::metrics::time_db("lesson_injection_stats", async move {
            let row = sqlx::query_as!(
                LessonInjectionSummary,
                r#"SELECT count(*) AS "count!", min(injected_at) AS first_injected_at,
                          max(injected_at) AS last_injected_at
                     FROM memory.lesson_injections
                    WHERE lesson_id = $1"#,
                lesson_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn lesson_went_live_at(
        self,
        lesson_id: i64,
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("lesson_went_live_at", async move {
            let subject_key = lesson_id.to_string();
            let changed_at = sqlx::query_scalar!(
                "SELECT changed_at FROM config.change_log
                  WHERE subject_kind = 'lesson_decision'
                    AND subject_key = $1
                    AND new_value ->> 'stage' = 'daily'
                  ORDER BY id ASC
                  LIMIT 1",
                subject_key,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(changed_at)
        })
        .await
    }
}
