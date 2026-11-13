use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct LessonDto {
    pub id: i64,
    pub project_id: Option<i64>,
    pub project_path: Option<String>,
    pub kind: String,
    pub text: String,
    pub source_turn_id: Option<i64>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LessonCreate {
    pub project_path: String,
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub source_turn_id: Option<i64>,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub supersedes: Option<i64>,
    #[serde(default)]
    pub topic: Option<String>,
}

pub const LESSON_KINDS: &[&str] = &["correction", "convention", "warning", "preference"];

pub fn lesson_error(status: StatusCode, error: &str) -> Response {
    (status, Json(serde_json::json!({ "error": error }))).into_response()
}

#[derive(Debug, Clone, Serialize)]
pub struct LessonRelatedSessionDto {
    pub session_id: String,
    pub title: Option<String>,
    pub similarity: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NearLessonDto {
    pub id: i64,
    pub text: String,
    pub similarity: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NeighbourLessonDto {
    pub id: i64,
    pub session_id: String,
    pub kind: String,
    pub stage: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LessonShapeDto {
    pub finding: &'static str,
    pub detail: String,
    pub chars: usize,
    pub max_chars: usize,
    pub held: bool,
    pub message: String,
}

impl LessonShapeDto {
    pub fn from_assessment(
        assessment: &crate::lesson_shape::Assessment,
        max_chars: usize,
        held: bool,
    ) -> Option<Self> {
        if assessment.is_rule() {
            return None;
        }
        let detail = &assessment.detail;
        let message = if held {
            format!(
                "Stored, but held at the raw stage and not injected: {detail}. It is promoted once it is rewritten as one durable rule (the imperative, the trigger, what to do, at most {max_chars} characters). The lesson proposer rewrites it on its next pass, or record a corrected lesson that supersedes this one."
            )
        } else {
            format!(
                "Stored and injected as written, but it does not read as a rule: {detail}. The lesson proposer is off, so nothing will rewrite it; restate it as one durable rule of at most {max_chars} characters and record that instead."
            )
        };
        Some(Self {
            finding: assessment.shape.code(),
            detail: assessment.detail.clone(),
            chars: assessment.chars,
            max_chars,
            held,
            message,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateLessonResponse {
    #[serde(flatten)]
    pub lesson: LessonDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<LessonShapeDto>,
    pub related_sessions: Vec<LessonRelatedSessionDto>,
    pub neighbour_lessons: Vec<NeighbourLessonDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub near_lessons: Vec<NearLessonDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_warning: Option<String>,
}
