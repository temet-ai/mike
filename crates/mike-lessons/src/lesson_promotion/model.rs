use serde::Deserialize;
use uuid::Uuid;

pub const MARKER: &str = "<!-- mike-learning-generated:v1 -->";
pub const STANDING_FILE: &str = ".claude/rules/mike-lessons.md";
pub const STANDING_CAP: usize = 60;

#[derive(Debug, Clone)]
pub struct LessonRow {
    pub id: i64,
    pub text: String,
    pub topic: Option<String>,
    pub source_turn_id: Option<i64>,
    pub stage: String,
    pub source: String,
    pub kind: String,
    pub project_id: i64,
    pub project_uuid: Uuid,
    pub project_path: String,
    pub agent_id: Option<i64>,
    pub agent_name: Option<String>,
    pub placement: Option<String>,
    pub covered_by: Option<String>,
    pub revision: i32,
}

#[derive(Debug, Clone)]
pub struct DraftRow {
    pub id: i64,
    pub project_uuid: Uuid,
    pub folder: String,
    pub lesson_watermark: i64,
    pub revision: i32,
    pub review: Option<String>,
    pub skill_md: String,
    pub reference_files: serde_json::Value,
    pub lesson_ids: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub description: String,
    pub cwd: String,
    pub contract: String,
}

pub(super) fn skill_key(project: &Uuid, folder: &str) -> String {
    format!("{project}:{folder}")
}

pub(super) fn key_project(key: &str) -> &str {
    key.split(':').next().unwrap_or_default()
}

pub(super) fn key_folder(key: &str) -> &str {
    key.split_once(':').map_or("", |(_, folder)| folder)
}
