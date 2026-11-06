use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};

use super::super::super::{PersistError, PersistHandle};

const EVENTS_SELECT: &str = "SELECT e.id, e.uuid, e.operator_turn_id, e.session_id, e.mode, e.status, e.arm,
        e.holdback_draw, e.arm_anchor_turn_id, e.due_at, e.reach_at_due, e.dispatched_at, e.verdict_json,
        e.outcome_label, e.label_source, e.label_evidence, e.trigger_point,
        i.uuid AS intervention_uuid, i.payload_text, i.delivery_channel, i.deliver_after, i.wake_state,
        i.pushed_at, i.acked_at, i.delivered_request_id, i.reverted_at, i.revert_reason
   FROM steering.evaluations e
   LEFT JOIN steering.interventions i ON i.uuid = e.intervention_uuid
  WHERE e.steerer_id = ";

fn events_query<'a>(f: &'a EventsFilter<'a>) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(EVENTS_SELECT);
    qb.push_bind(f.steerer_id)
        .push(" AND e.question = ")
        .push_bind(f.question)
        .push(" AND e.id < ")
        .push_bind(f.before_id.unwrap_or(i64::MAX));
    if let Some(arm) = f.arm {
        qb.push(" AND e.arm = ").push_bind(arm);
    }
    if let Some(mode) = f.mode {
        qb.push(" AND e.mode = ").push_bind(mode);
    }
    qb.push(" ORDER BY e.id DESC LIMIT ").push_bind(f.limit);
    qb
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventsFilter<'a> {
    pub steerer_id: i64,
    pub question: &'a str,
    pub arm: Option<&'a str>,
    pub mode: Option<&'a str>,
    pub before_id: Option<i64>,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct QuestionEventRow {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub operator_turn_id: i64,
    pub session_id: i64,
    pub mode: String,
    pub status: String,
    pub arm: Option<String>,
    pub holdback_draw: Option<f64>,
    pub arm_anchor_turn_id: Option<i64>,
    pub due_at: Option<DateTime<Utc>>,
    pub reach_at_due: Option<String>,
    pub dispatched_at: DateTime<Utc>,
    pub verdict_json: Option<serde_json::Value>,
    pub outcome_label: Option<String>,
    pub label_source: Option<String>,
    pub label_evidence: Option<serde_json::Value>,
    pub trigger_point: Option<String>,
    pub intervention_uuid: Option<uuid::Uuid>,
    pub payload_text: Option<String>,
    pub delivery_channel: Option<String>,
    pub deliver_after: Option<DateTime<Utc>>,
    pub wake_state: Option<String>,
    pub pushed_at: Option<DateTime<Utc>>,
    pub acked_at: Option<DateTime<Utc>>,
    pub delivered_request_id: Option<i64>,
    pub reverted_at: Option<DateTime<Utc>>,
    pub revert_reason: Option<String>,
}

impl PersistHandle {
    pub async fn steer_question_events(
        &self,
        f: &EventsFilter<'_>,
    ) -> Result<Vec<QuestionEventRow>, PersistError> {
        crate::metrics::time_db("steer_question_events", async move {
            Ok(events_query(f)
                .build_query_as::<QuestionEventRow>()
                .fetch_all(&self.pool)
                .await?)
        })
        .await
    }
}
