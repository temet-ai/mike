use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::config::AppState;
use crate::lesson_delivery::{self, Arm, DeliveredLesson, PromptLessons};
use crate::recall_injector::formatter;

#[derive(Debug, Deserialize)]
pub(crate) struct ForPromptRequest {
    session: String,
    project: String,
    prompt: String,
    #[serde(default)]
    held: Vec<i64>,
    #[serde(default)]
    held_versions: Option<std::collections::HashMap<i64, String>>,
    #[serde(default)]
    refresh: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ForPromptResponse {
    arm: &'static str,
    lessons: Vec<DeliveredLesson>,
    topic_catalog: Option<String>,
    memory_grammar: Option<&'static str>,
    memory_topics: Vec<String>,
}

fn answer(arm: &'static str, lessons: Vec<DeliveredLesson>) -> Json<ForPromptResponse> {
    Json(ForPromptResponse {
        arm,
        lessons,
        topic_catalog: None,
        memory_grammar: None,
        memory_topics: Vec::new(),
    })
}

pub(crate) async fn post_for_prompt(
    State(state): State<AppState>,
    Json(request): Json<ForPromptRequest>,
) -> Result<Json<ForPromptResponse>, StatusCode> {
    if uuid::Uuid::parse_str(&request.session).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let toggles = state.feature_toggle_overlay.load();
    let settings = state.settings_overlay.load();
    if !toggles.mod_lesson_delivery || !settings.reminders_inject_lessons {
        return Ok(answer("off", Vec::new()));
    }
    let arm = lesson_delivery::arm_for(&request.session);
    let mut memory_topics = None;
    let lessons = match arm {
        Arm::Control => Vec::new(),
        Arm::Mod => {
            let snapshot = state.lesson_cache.load();
            if request.refresh {
                snapshot.relevance.forget_session(&request.session);
            }
            snapshot.relevance.note_mod_session(&request.session);
            if toggles.recall_injector {
                memory_topics = Some(lesson_delivery::catalog_for_prompt(
                    &snapshot,
                    &request.session,
                    &request.project,
                    &request.prompt,
                ));
            }
            lesson_delivery::select_for_prompt(PromptLessons {
                snapshot: &snapshot,
                session_id: &request.session,
                project_path: &request.project,
                prompt: &request.prompt,
                held: &request.held,
                held_versions: request.held_versions.as_ref(),
                limit: settings.reminders_max_lessons_to_inject,
                content_only: toggles.recall_injector,
            })
        }
    };
    record_arm(&state, &request.session, arm, &lessons).await;
    Ok(Json(ForPromptResponse {
        arm: arm.as_str(),
        lessons,
        topic_catalog: memory_topics
            .as_deref()
            .and_then(|topics| formatter::memory_note(topics, true)),
        memory_grammar: memory_topics
            .is_some()
            .then_some(formatter::MEMORY_TAG_GRAMMAR),
        memory_topics: memory_topics.unwrap_or_default(),
    }))
}

async fn record_arm(state: &AppState, session: &str, arm: Arm, lessons: &[DeliveredLesson]) {
    let Some(persist) = &state.persist else {
        return;
    };
    let bytes: usize = lessons.iter().map(|lesson| lesson.text.len()).sum();
    let count = i32::try_from(lessons.len()).unwrap_or(i32::MAX);
    let bytes = i64::try_from(bytes).unwrap_or(i64::MAX);
    if let Err(error) = persist
        .record_lesson_delivery(session, arm.as_str(), count, bytes)
        .await
    {
        tracing::warn!(session_id = %session, %error, "lesson delivery arm write failed");
    }
}
