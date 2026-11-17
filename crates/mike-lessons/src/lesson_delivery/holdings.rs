use crate::store::persistence::lessons_repo::LessonsPersist as _;
use chrono::{TimeDelta, Utc};

use crate::lesson_cache::LessonRelevance;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

const RESTORE_DAYS: i64 = 7;

pub fn spawn_record_holdings(
    state: &AppState,
    session: &str,
    route: &'static str,
    fresh: Vec<(i64, i32)>,
) {
    let Some(persist) = state.persist.clone() else {
        return;
    };
    if fresh.is_empty() {
        return;
    }
    let session = session.to_owned();
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        if let Err(error) = persist
            .lessons()
            .record_lesson_holdings(&session, route, &fresh)
            .await
        {
            tracing::warn!(session_id = %session, %error, "lesson holdings write failed");
        }
    });
}

pub fn spawn_forget_holdings(state: &AppState, session: &str) {
    let Some(persist) = state.persist.clone() else {
        return;
    };
    let session = session.to_owned();
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        if let Err(error) = persist.lessons().forget_lesson_holdings(&session).await {
            tracing::warn!(session_id = %session, %error, "lesson holdings release failed");
        }
    });
}

pub fn spawn_record_call(state: &AppState, session: &str, lessons: i32, bytes: i64) {
    let Some(persist) = state.persist.clone() else {
        return;
    };
    let session = session.to_owned();
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        if let Err(error) = persist
            .lessons()
            .record_lesson_delivery(&session, lessons, bytes)
            .await
        {
            tracing::warn!(session_id = %session, %error, "lesson delivery write failed");
        }
    });
}

pub async fn restore(persist: &PersistHandle, relevance: &LessonRelevance) -> bool {
    let since = Utc::now() - TimeDelta::days(RESTORE_DAYS);
    let clients = persist.lessons().recent_client_sessions(since).await;
    let held = persist.lessons().recent_lesson_holdings(since).await;
    let declared = persist.lessons().recent_declared_sessions(since).await;
    match (clients, held, declared) {
        (Ok(clients), Ok(held), Ok(declared)) => {
            tracing::info!(
                clients = clients.len(),
                held = held.len(),
                declared = declared.len(),
                "lesson delivery: client sessions and held lessons restored"
            );
            relevance.holdings().restore(clients, held);
            relevance.holdings().restore_declared(declared);
            true
        }
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
            tracing::warn!(%error, "lesson delivery: restoring client sessions failed");
            false
        }
    }
}

pub fn declare_client(state: &AppState, session: &str) {
    let snapshot = state.lesson_cache.load();
    if !snapshot.relevance.holdings().note_declared(session) {
        return;
    }
    let Some(persist) = state.persist.clone() else {
        return;
    };
    let session = session.to_owned();
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        if let Err(error) = persist
            .lessons()
            .record_lesson_client_declared(&session)
            .await
        {
            tracing::warn!(session_id = %session, %error, "lesson client declaration write failed");
        }
    });
}
