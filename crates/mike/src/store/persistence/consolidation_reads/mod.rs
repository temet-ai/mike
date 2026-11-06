mod queries;

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use uuid::Uuid;

use super::lessons::LessonRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct RunnerUp {
    pub(crate) lesson_id: i64,
    pub(crate) distance: f32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ConsolidationRunsFilter {
    pub(crate) kind: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) session: Option<Uuid>,
    pub(crate) file: Option<String>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ConsolidationDecisionsFilter {
    pub(crate) project: Option<String>,
    pub(crate) session: Option<Uuid>,
    pub(crate) file: Option<String>,
    pub(crate) action_kind: Option<String>,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct ConsolidationRunRow {
    pub(crate) id: i64,
    pub(crate) uuid: String,
    pub(crate) kind: String,
    pub(crate) trigger: String,
    pub(crate) status: String,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) finished_at: Option<DateTime<Utc>>,
    pub(crate) error: Option<String>,
    pub(crate) action_count: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct ConsolidationActionRow {
    pub(crate) id: i64,
    pub(crate) run_id: i64,
    pub(crate) run_kind: String,
    pub(crate) lesson_id: Option<i64>,
    pub(crate) lesson_uuid: String,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: Option<String>,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) source_turn_id: Option<i64>,
    pub(crate) files_touched: Option<Vec<String>>,
    pub(crate) agent_id: Option<i64>,
    pub(crate) lesson_text: String,
    pub(crate) action_kind: String,
    pub(crate) outcome: String,
    pub(crate) error: Option<String>,
    pub(crate) before_stage: Option<String>,
    pub(crate) after_stage: Option<String>,
    pub(crate) decided_by: Option<String>,
    pub(crate) distance: Option<f32>,
    pub(crate) threshold: Option<f32>,
    pub(crate) embedding_model: Option<String>,
    pub(crate) counterpart_lesson_id: Option<i64>,
    pub(crate) counterpart_uuid: Option<String>,
    pub(crate) counterpart_text: Option<String>,
    pub(crate) runners_up: Json<Vec<RunnerUp>>,
    pub(crate) rank_detail: Option<Json<serde_json::Value>>,
    pub(crate) injection_count: Option<i32>,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct LessonInjectionRow {
    pub(crate) id: i64,
    pub(crate) lesson_id: i64,
    pub(crate) request_id: i64,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) injected_at: DateTime<Utc>,
    pub(crate) lane: String,
    pub(crate) persona: Option<String>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct ArchivedLessonRow {
    pub(crate) id: i64,
    pub(crate) uuid: String,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: Option<String>,
    pub(crate) kind: String,
    pub(crate) text: String,
    pub(crate) topic: Option<String>,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) source_turn_id: Option<i64>,
    pub(crate) agent_id: Option<i64>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) archived_at: Option<DateTime<Utc>>,
    pub(crate) archived_reason: Option<String>,
    pub(crate) duplicate_of_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct LessonHistoryHeaderRow {
    pub(crate) lesson_id: i64,
    pub(crate) lesson_uuid: String,
    pub(crate) lesson_text: String,
    pub(crate) kind: String,
    pub(crate) stage: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) source_turn_id: Option<i64>,
}

pub(crate) struct LessonHistory {
    pub(crate) header: LessonHistoryHeaderRow,
    pub(crate) actions: Vec<ConsolidationActionRow>,
    pub(crate) injections: Vec<LessonInjectionRow>,
}

pub(crate) enum RestoreLessonOutcome {
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
