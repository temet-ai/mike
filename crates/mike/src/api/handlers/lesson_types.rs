use super::*;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LessonDto {
    pub(crate) id: i64,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: Option<String>,
    pub(crate) kind: String,
    pub(crate) text: String,
    pub(crate) source_turn_id: Option<i64>,
    pub(crate) created_at: String,
    pub(crate) expires_at: Option<String>,
    pub(crate) topic: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LessonCreate {
    pub(crate) project_path: String,
    pub(crate) kind: String,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) source_turn_id: Option<i64>,
    #[serde(default)]
    pub(crate) agent_name: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) supersedes: Option<i64>,
    #[serde(default)]
    pub(crate) topic: Option<String>,
}

pub(crate) const LESSON_KINDS: &[&str] = &["correction", "convention", "warning", "preference"];

pub(crate) fn lesson_error(status: StatusCode, error: &str) -> Response {
    (status, Json(serde_json::json!({ "error": error }))).into_response()
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LessonRelatedSessionDto {
    pub(crate) session_id: String,
    pub(crate) title: Option<String>,
    pub(crate) similarity: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NearLessonDto {
    pub(crate) id: i64,
    pub(crate) text: String,
    pub(crate) similarity: f64,
    pub(crate) source: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NeighbourLessonDto {
    pub(crate) id: i64,
    pub(crate) session_id: String,
    pub(crate) kind: String,
    pub(crate) stage: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LessonShapeDto {
    pub(crate) finding: &'static str,
    pub(crate) detail: String,
    pub(crate) chars: usize,
    pub(crate) max_chars: usize,
    pub(crate) held: bool,
    pub(crate) message: String,
}

impl LessonShapeDto {
    pub(crate) fn from_assessment(
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
pub(crate) struct CreateLessonResponse {
    #[serde(flatten)]
    pub(crate) lesson: LessonDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) shape: Option<LessonShapeDto>,
    pub(crate) related_sessions: Vec<LessonRelatedSessionDto>,
    pub(crate) neighbour_lessons: Vec<NeighbourLessonDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) near_lessons: Vec<NearLessonDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repo_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repo_warning: Option<String>,
}
