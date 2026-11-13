use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMode {
    Shadow,
    On,
    Replay,
    Cli,
}

impl DecisionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::On => "on",
            Self::Replay => "replay",
            Self::Cli => "cli",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DecisionSubject {
    pub session_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub request_id: Option<i64>,
    pub evaluation_uuid: Option<Uuid>,
    pub intervention_uuid: Option<Uuid>,
    pub fleet_run_id: Option<i64>,
    pub seat: Option<String>,
    pub message_uuid: Option<Uuid>,
    pub check_receipt_id: Option<i64>,
    pub commit_sha: Option<String>,
    pub project_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionContext {
    pub point: &'static str,
    pub mode: DecisionMode,
    pub question_set: &'static str,
    pub thresholds: Value,
    pub facts_version: Option<i32>,
    pub facts: Option<Value>,
    pub subject: DecisionSubject,
    pub source_ref: Option<String>,
}

impl DecisionContext {
    pub fn new(point: &'static str, mode: DecisionMode, question_set: &'static str) -> Self {
        Self {
            point,
            mode,
            question_set,
            thresholds: Value::Object(serde_json::Map::new()),
            facts_version: None,
            facts: None,
            subject: DecisionSubject::default(),
            source_ref: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallResult {
    pub outcome: &'static str,
    pub error: Option<String>,
    pub response_raw: Option<String>,
    pub answers: Option<Value>,
    pub question_refusals: i32,
    pub served_model: Option<String>,
    pub provider: Option<String>,
    pub generation_id: Option<String>,
    pub usage: Option<Value>,
    pub cost_usd: Option<f64>,
    pub latency_ms: Option<i32>,
    pub http_status: Option<i32>,
}

pub struct RecordInsert<'a> {
    pub context: &'a DecisionContext,
    pub model_requested: &'a str,
    pub state: &'a Value,
    pub questions: &'a Value,
    pub request_body: &'a Value,
    pub result: &'a CallResult,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Applied {
    pub composed: Value,
    pub applied: &'static str,
    pub fail_open: bool,
}

pub fn question_set_hash(questions: &Value) -> String {
    let digest = Sha256::digest(questions.to_string().as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

impl PersistHandle {
    pub async fn insert_decision_record(&self, row: RecordInsert<'_>) -> Result<i64, PersistError> {
        let machine_uuid = self.llm_machine_uuid().await;
        let ctx = row.context;
        let s = &ctx.subject;
        let r = row.result;
        let hash = question_set_hash(row.questions);
        crate::metrics::time_db("insert_decision_record", async move {
            Ok(sqlx::query_scalar!(
                r#"INSERT INTO decisions.records
                       (created_at, point, mode, question_set, question_set_hash, thresholds,
                        model_requested, state, questions, request_body, outcome, error,
                        response_raw, answers, question_refusals, served_model, provider,
                        generation_id, usage, cost_usd, latency_ms, http_status, facts_version,
                        facts, session_id, operator_turn_id, request_id, evaluation_uuid,
                        intervention_uuid, fleet_run_id, seat, message_uuid, check_receipt_id,
                        commit_sha, project_id, machine_uuid, source_ref)
                   VALUES (COALESCE($1, now()), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                           $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26,
                           $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37)
                   RETURNING id"#,
                row.created_at,
                ctx.point,
                ctx.mode.as_str(),
                ctx.question_set,
                hash,
                ctx.thresholds,
                row.model_requested,
                row.state,
                row.questions,
                row.request_body,
                r.outcome,
                r.error,
                r.response_raw,
                r.answers,
                r.question_refusals,
                r.served_model,
                r.provider,
                r.generation_id,
                r.usage,
                r.cost_usd,
                r.latency_ms,
                r.http_status,
                ctx.facts_version,
                ctx.facts,
                s.session_id,
                s.operator_turn_id,
                s.request_id,
                s.evaluation_uuid,
                s.intervention_uuid,
                s.fleet_run_id,
                s.seat,
                s.message_uuid,
                s.check_receipt_id,
                s.commit_sha,
                s.project_id,
                machine_uuid,
                ctx.source_ref,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn mark_decision_applied(
        &self,
        record_id: i64,
        applied: &Applied,
        evaluation_uuid: Option<Uuid>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("mark_decision_applied", async move {
            sqlx::query!(
                "UPDATE decisions.records
                    SET composed = $2, applied = $3, fail_open = $4,
                        evaluation_uuid = COALESCE($5, evaluation_uuid)
                  WHERE id = $1",
                record_id,
                applied.composed,
                applied.applied,
                applied.fail_open,
                evaluation_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn insert_decision_outcome(
        &self,
        record_id: i64,
        label: &str,
        label_source: &str,
        evidence: &Value,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("insert_decision_outcome", async move {
            let r = sqlx::query!(
                "INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (record_id, label_source, label) DO NOTHING",
                record_id,
                label,
                label_source,
                evidence,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected() > 0)
        })
        .await
    }

    pub async fn label_decisions_for_evaluations(
        &self,
        point: &str,
        labels: &[(Uuid, String)],
        label_source: &str,
    ) -> Result<u64, PersistError> {
        let uuids: Vec<Uuid> = labels.iter().map(|(u, _)| *u).collect();
        let values: Vec<String> = labels.iter().map(|(_, l)| l.clone()).collect();
        crate::metrics::time_db("label_decisions_for_evaluations", async move {
            let r = sqlx::query!(
                "INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
                 SELECT r.id, l.label, $3, jsonb_build_object('evaluation_uuid', l.uuid)
                   FROM unnest($1::uuid[], $2::text[]) AS l(uuid, label)
                   JOIN decisions.records r
                     ON r.evaluation_uuid = l.uuid AND r.point = $4
                 ON CONFLICT (record_id, label_source, label) DO NOTHING",
                &uuids,
                &values,
                label_source,
                point,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected())
        })
        .await
    }
}
