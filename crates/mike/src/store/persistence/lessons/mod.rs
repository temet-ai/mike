mod consolidation;
mod detail;
mod discovery;
pub mod harvest_runs;
mod injectable;
pub mod maintenance_budget;
#[cfg(test)]
mod maintenance_budget_tests;
pub mod placement;
pub mod placement_proposals;
pub mod proposals;
#[cfg(test)]
mod proposals_tests;
mod reads;
mod retention;
pub mod shape;
pub mod shape_apply;
mod skill_coverage;
pub mod skill_draft_eval;
pub mod skill_drafts;
pub mod skill_live;
pub mod skill_review;
pub mod skill_review_detail;
pub mod skill_review_writes;
pub mod skill_verdicts;
pub mod topics;
mod vector_repair;
mod vectors;
pub mod write_time;
mod writes;

pub use detail::{LessonEvidenceRow, LessonInjectionSummary};
pub use injectable::{ChangeMark, InjectableLessonRow};

pub struct LessonRow {
    pub id: i64,
    pub project_id: Option<i64>,
    pub project_path: Option<String>,
    pub kind: String,
    pub text: String,
    pub agent_id: Option<i64>,
    pub stage: String,
    pub source_turn_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub topic: Option<String>,
}

pub struct LessonInsertRow {
    pub id: i64,
    #[allow(dead_code)]
    pub uuid: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct NewLessonWrite<'a> {
    pub project_id: i64,
    pub kind: &'a str,
    pub text: &'a str,
    pub topic: Option<&'a str>,
    pub source_turn_id: Option<i64>,
    pub agent_id: Option<i64>,
    pub session_db_id: Option<i64>,
    pub request_id: Option<i64>,
}

pub struct SimilarLesson {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub text: String,
    pub distance: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LessonCandidateRow {
    pub id: i64,
    pub project_id: Option<i64>,
    pub project_path: Option<String>,
    pub kind: String,
    pub text: String,
    pub source: String,
    pub stage: String,
    pub source_turn_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub evidence_count: i64,
    pub session_uuid: Option<uuid::Uuid>,
    pub session_title: Option<String>,
    pub turn_closed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub user_prompt: Option<String>,
    pub final_assistant_text: Option<String>,
    pub correcting_message: Option<String>,
    pub carry_runs_given: Option<i32>,
    pub carry_later_findings: Option<i32>,
}
