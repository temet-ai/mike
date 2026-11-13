use crate::store::persistence::lessons_repo::LessonsPersist as _;

use crate::state::AppState;
use crate::state::lesson_hooks::LessonAttribution;
use crate::store::models::RequestId;

pub fn spawn_attribution(state: &AppState, attribution: LessonAttribution<'_>) {
    spawn_mod_lesson_attribution(
        state,
        attribution.session_id,
        attribution.lane,
        attribution.mod_lessons,
        attribution.request_id,
    );
    spawn_lesson_injection_attribution(
        state,
        attribution.session_id,
        attribution.lane,
        attribution.persona,
        attribution.injected,
        attribution.request_id,
    );
}

pub fn spawn_lesson_injection_attribution(
    state: &AppState,
    session_id: &str,
    lesson_injection_lane: &'static str,
    lesson_injection_persona: Option<String>,
    injected_lesson_ids: Vec<i64>,
    request_id: RequestId,
) {
    if injected_lesson_ids.is_empty() {
        return;
    }
    let Some(persist) = &state.persist else {
        return;
    };
    let persist_for_lessons = persist.clone();
    let session_id_for_lessons = session_id.to_string();
    let db_request_id = request_id.as_u64() as i64;
    crate::observability::spans::spawn_detached(async move {
        if let Err(e) = persist_for_lessons
            .lessons()
            .record_lesson_injections(
                db_request_id,
                &session_id_for_lessons,
                lesson_injection_lane,
                lesson_injection_persona.as_deref(),
                &injected_lesson_ids,
            )
            .await
        {
            tracing::warn!(
                request_id = db_request_id,
                error = %e,
                "lesson injection attribution write failed"
            );
        }
    });
}

pub fn spawn_mod_lesson_attribution(
    state: &AppState,
    session_id: &str,
    lesson_injection_lane: &'static str,
    mod_lesson_ids: Vec<i64>,
    request_id: RequestId,
) {
    if mod_lesson_ids.is_empty() {
        return;
    }
    let Some(persist) = &state.persist else {
        return;
    };
    let persist = persist.clone();
    let session_id = session_id.to_string();
    let db_request_id = request_id.as_u64() as i64;
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        if let Err(e) = persist
            .lessons()
            .record_mod_lesson_injections(
                db_request_id,
                &session_id,
                lesson_injection_lane,
                &mod_lesson_ids,
            )
            .await
        {
            tracing::warn!(
                request_id = db_request_id,
                error = %e,
                "mod lesson attribution write failed"
            );
        }
    });
}
