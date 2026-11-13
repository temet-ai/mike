pub mod queries;

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use uuid::Uuid;

use super::lessons::LessonRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunnerUp {
    pub lesson_id: i64,
    pub distance: f32,
}

#[derive(Debug, Clone, Default)]
pub struct ConsolidationRunsFilter {
    pub kind: Option<String>,
    pub status: Option<String>,
    pub project: Option<String>,
    pub session: Option<Uuid>,
    pub file: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ConsolidationDecisionsFilter {
    pub project: Option<String>,
    pub session: Option<Uuid>,
    pub file: Option<String>,
    pub action_kind: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(sqlx::FromRow)]
pub struct ConsolidationRunRow {
    pub id: i64,
    pub uuid: String,
    pub kind: String,
    pub trigger: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub action_count: i64,
}

#[derive(sqlx::FromRow)]
pub struct ConsolidationActionRow {
    pub id: i64,
    pub run_id: i64,
    pub run_kind: String,
    pub lesson_id: Option<i64>,
    pub lesson_uuid: String,
    pub project_id: Option<i64>,
    pub project_path: Option<String>,
    pub session_id: Option<i64>,
    pub session_uuid: Option<String>,
    pub source_turn_id: Option<i64>,
    pub files_touched: Option<Vec<String>>,
    pub agent_id: Option<i64>,
    pub lesson_text: String,
    pub action_kind: String,
    pub outcome: String,
    pub error: Option<String>,
    pub before_stage: Option<String>,
    pub after_stage: Option<String>,
    pub decided_by: Option<String>,
    pub distance: Option<f32>,
    pub threshold: Option<f32>,
    pub embedding_model: Option<String>,
    pub counterpart_lesson_id: Option<i64>,
    pub counterpart_uuid: Option<String>,
    pub counterpart_text: Option<String>,
    pub runners_up: Json<Vec<RunnerUp>>,
    pub rank_detail: Option<Json<serde_json::Value>>,
    pub injection_count: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct LessonInjectionRow {
    pub id: i64,
    pub lesson_id: i64,
    pub request_id: i64,
    pub session_id: Option<i64>,
    pub session_uuid: Option<String>,
    pub injected_at: DateTime<Utc>,
    pub lane: String,
    pub persona: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct ArchivedLessonRow {
    pub id: i64,
    pub uuid: String,
    pub project_id: Option<i64>,
    pub project_path: Option<String>,
    pub kind: String,
    pub text: String,
    pub topic: Option<String>,
    pub session_id: Option<i64>,
    pub session_uuid: Option<String>,
    pub source_turn_id: Option<i64>,
    pub agent_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub archived_reason: Option<String>,
    pub duplicate_of_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub struct LessonHistoryHeaderRow {
    pub lesson_id: i64,
    pub lesson_uuid: String,
    pub lesson_text: String,
    pub kind: String,
    pub stage: String,
    pub created_at: DateTime<Utc>,
    pub session_uuid: Option<String>,
    pub source_turn_id: Option<i64>,
}

pub struct LessonHistory {
    pub header: LessonHistoryHeaderRow,
    pub actions: Vec<ConsolidationActionRow>,
    pub injections: Vec<LessonInjectionRow>,
}

pub enum RestoreLessonOutcome {
    NotFound,
    NotArchived,
    Restored(LessonRow),
}

#[derive(sqlx::FromRow)]
struct RestoredLessonRow {
    id: i64,
    project_id: Option<i64>,
    project_path: Option<String>,
    kind: String,
    text: String,
    agent_id: Option<i64>,
    stage: String,
    source_turn_id: Option<i64>,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    topic: Option<String>,
}

impl From<RestoredLessonRow> for LessonRow {
    fn from(r: RestoredLessonRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            project_path: r.project_path,
            kind: r.kind,
            text: r.text,
            agent_id: r.agent_id,
            stage: r.stage,
            source_turn_id: r.source_turn_id,
            created_at: r.created_at,
            expires_at: r.expires_at,
            topic: r.topic,
        }
    }
}

const ACTION_SELECT: &str = "a.id, a.run_id, rk.code AS run_kind,
    a.lesson_id, a.lesson_uuid::text AS lesson_uuid, a.project_id, p.project_path,
    a.session_id, a.session_uuid::text AS session_uuid, a.source_turn_id,
    a.files_touched, a.agent_id, a.lesson_text,
    ak.code AS action_kind, oc.code AS outcome, a.error,
    bs.code AS before_stage, as_.code AS after_stage, db.code AS decided_by,
    a.distance, a.threshold, a.embedding_model,
    a.counterpart_lesson_id, a.counterpart_uuid::text AS counterpart_uuid, a.counterpart_text,
    a.runners_up, a.rank_detail, a.injection_count, a.created_at";

const ACTION_FROM: &str = "memory.consolidation_actions a
    JOIN memory.consolidation_runs r ON r.id = a.run_id
    JOIN memory.consolidation_run_kinds rk ON rk.id = r.run_kind_id
    LEFT JOIN projects.projects p ON p.id = a.project_id
    JOIN memory.consolidation_action_kinds ak ON ak.id = a.action_kind_id
    JOIN memory.consolidation_outcomes oc ON oc.id = a.outcome_id
    LEFT JOIN memory.lesson_stages bs ON bs.id = a.before_stage_id
    LEFT JOIN memory.lesson_stages as_ ON as_.id = a.after_stage_id
    LEFT JOIN memory.consolidation_decided_by db ON db.id = a.decided_by_id";

const RUN_SELECT: &str = "r.id, r.uuid::text AS uuid, rk.code AS kind, tr.code AS trigger,
    st.code AS status, r.started_at, r.finished_at, r.error,
    COALESCE(ac.action_count, 0) AS action_count";

const RUN_FROM: &str = "memory.consolidation_runs r
    JOIN memory.consolidation_run_kinds rk ON rk.id = r.run_kind_id
    JOIN memory.consolidation_triggers tr ON tr.id = r.trigger_id
    JOIN memory.consolidation_run_statuses st ON st.id = r.status_id
    LEFT JOIN (
        SELECT run_id, count(*)::bigint AS action_count
          FROM memory.consolidation_actions
         GROUP BY run_id
    ) ac ON ac.run_id = r.id";
