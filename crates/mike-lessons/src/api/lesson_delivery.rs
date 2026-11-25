use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::lesson_delivery::holdings::{
    spawn_forget_holdings, spawn_record_call, spawn_record_holdings,
};
use crate::lesson_delivery::prompt_context::PromptContext;
use crate::lesson_delivery::{self, CLIENT_ROUTE, DeliveredLesson, PromptLessons};
use crate::lesson_worker::LessonSnapshot;
use crate::lesson_worker::relevance::embed_now;
use crate::memory_keeper::block::Block;
use crate::memory_keeper::deliver::{CardAsk, card_for, serves_cards};
use crate::memory_keeper::supply::Carrier;
use crate::recall_injector::formatter;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ForPromptRequest {
    session: String,
    project: String,
    prompt: String,
    #[serde(default)]
    held: Vec<i64>,
    #[serde(default)]
    held_versions: Option<std::collections::HashMap<i64, String>>,
    #[serde(default)]
    refresh: bool,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default = "wants_card")]
    card: bool,
}

fn wants_card() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct ForPromptResponse {
    arm: &'static str,
    lessons: Vec<DeliveredLesson>,
    topic_catalog: Option<String>,
    memory_grammar: Option<&'static str>,
    memory_topics: Vec<String>,
    memory_keeper: Option<String>,
    memory_keeper_hash: Option<String>,
}

struct Lessons {
    arm: &'static str,
    lessons: Vec<DeliveredLesson>,
    memory_topics: Option<Vec<String>>,
}

pub async fn post_for_prompt(
    State(state): State<AppState>,
    Json(request): Json<ForPromptRequest>,
) -> Result<Json<ForPromptResponse>, StatusCode> {
    let Ok(session) = Uuid::parse_str(&request.session) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let context = PromptContext::load(state.persist(), session).await;
    let query = match_text(&state, &context, &request.prompt).await;
    let mut lessons = lessons(&state, &request, &query);
    lessons.lessons = above_floor(&state, &request, context, lessons.lessons).await;
    if lessons.arm == CLIENT_ROUTE {
        let snapshot = state.lesson_cache.load();
        lesson_delivery::mark_delivered(
            &snapshot,
            &request.session,
            &request.project,
            &lessons.lessons,
        );
        record_given(&state, &snapshot, &request.session, &lessons.lessons);
    }
    let card = if request.card {
        card(&state, session, &request).await
    } else {
        None
    };
    let topics = lessons.memory_topics;
    Ok(Json(ForPromptResponse {
        arm: lessons.arm,
        lessons: lessons.lessons,
        topic_catalog: topics
            .as_deref()
            .and_then(|topics| formatter::memory_note(topics, true)),
        memory_grammar: topics.is_some().then_some(formatter::MEMORY_TAG_GRAMMAR),
        memory_topics: topics.unwrap_or_default(),
        memory_keeper_hash: card.as_ref().map(|block| block.hash.clone()),
        memory_keeper: card.map(|block| block.text),
    }))
}

async fn match_text(state: &AppState, context: &PromptContext, prompt: &str) -> String {
    let query = context.query(prompt);
    let snapshot = state.lesson_cache.load_full();
    if embed_now(&snapshot.relevance, &state.embedding_service, &query).await {
        query
    } else {
        prompt.to_owned()
    }
}

async fn above_floor(
    state: &AppState,
    request: &ForPromptRequest,
    earlier: PromptContext,
    chosen: Vec<DeliveredLesson>,
) -> Vec<DeliveredLesson> {
    use crate::reminders::supply_floor::{self, Candidate, Context};
    let Some(decider) = state
        .decider(supply_floor::POINT)
        .map(crate::llm::decisions::point::Decider::inline)
    else {
        return chosen;
    };
    let asked: Vec<Candidate<'_>> = chosen
        .iter()
        .map(|l| Candidate {
            id: l.id,
            kind: &l.kind,
            text: &l.text,
        })
        .collect();
    let context = Context {
        session: request.session.as_str(),
        request_id: 0,
        text: &request.prompt,
        earlier,
    };
    let kept = supply_floor::apply(&decider, &context, &asked).await;
    chosen
        .into_iter()
        .filter(|l| kept.contains(&l.id))
        .collect()
}

async fn card(state: &AppState, session: Uuid, request: &ForPromptRequest) -> Option<Block> {
    if !serves_cards(state) {
        return None;
    }
    card_for(
        state,
        &CardAsk {
            session,
            project_path: &request.project,
            prompt: Some(&request.prompt),
            cwd: request.cwd.as_deref(),
            carrier: Carrier::Mod,
        },
    )
    .await
}

fn lessons(state: &AppState, request: &ForPromptRequest, query: &str) -> Lessons {
    let toggles = state.feature_toggle_overlay.load();
    let settings = state
        .settings_overlay
        .load()
        .features
        .get::<crate::features_settings::RemindersOverlay>();
    if !toggles.mod_lesson_delivery || !settings.inject_lessons {
        return Lessons {
            arm: "off",
            lessons: Vec::new(),
            memory_topics: None,
        };
    }
    let snapshot = state.lesson_cache.load();
    let session = request.session.as_str();
    snapshot.relevance.holdings().note_client(session);
    track_client_holdings(state, &snapshot, request);
    let memory_topics = toggles.recall_injector.then(|| {
        lesson_delivery::catalog_for_prompt(&snapshot, session, &request.project, &request.prompt)
    });
    let lessons = lesson_delivery::select_for_prompt(PromptLessons {
        snapshot: &snapshot,
        session_id: session,
        project_path: &request.project,
        prompt: query,
        held: &request.held,
        held_versions: request.held_versions.as_ref(),
        held_tags: &[],
        limit: settings.max_lessons_to_inject,
        content_only: toggles.recall_injector,
    });
    Lessons {
        arm: CLIENT_ROUTE,
        lessons,
        memory_topics,
    }
}

fn track_client_holdings(state: &AppState, snapshot: &LessonSnapshot, request: &ForPromptRequest) {
    let session = request.session.as_str();
    if request.refresh {
        snapshot.relevance.forget_session(session);
        snapshot.relevance.holdings().forget(session);
        spawn_forget_holdings(state, session);
        return;
    }
    let Some(versions) = request.held_versions.as_ref() else {
        return;
    };
    let held = lesson_delivery::client_held(snapshot, session, &request.project, versions);
    let fresh = snapshot.relevance.holdings().hold(session, &held);
    spawn_record_holdings(state, session, CLIENT_ROUTE, fresh);
}

fn record_given(
    state: &AppState,
    snapshot: &LessonSnapshot,
    session: &str,
    lessons: &[DeliveredLesson],
) {
    let given: Vec<(i64, i32)> = lessons
        .iter()
        .map(|lesson| (lesson.id, lesson.rev))
        .collect();
    let fresh = snapshot.relevance.holdings().hold(session, &given);
    spawn_record_holdings(state, session, CLIENT_ROUTE, fresh);
    let bytes: usize = lessons.iter().map(|lesson| lesson.text.len()).sum();
    spawn_record_call(
        state,
        session,
        i32::try_from(lessons.len()).unwrap_or(i32::MAX),
        i64::try_from(bytes).unwrap_or(i64::MAX),
    );
}
