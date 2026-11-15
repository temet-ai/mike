use uuid::Uuid;

use super::topic::{self, NearMissContext};
use super::{RecallInjectorConfig, formatter, list_rows, order_by_requested_topics, topic_memory};
use crate::llm::decisions::env::DecisionEnv;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::LessonRow;

#[derive(Debug, Default)]
pub struct RecallAnswer {
    pub lesson_ids: Vec<i64>,
    pub text: Option<String>,
}

#[derive(Clone)]
pub struct NearMissLearning {
    pub env: DecisionEnv,
    pub session: Option<Uuid>,
}

pub async fn answer_recall(
    persist: &PersistHandle,
    project_path: &str,
    topics: &[String],
    cfg: &RecallInjectorConfig,
) -> RecallAnswer {
    answer_recall_with(persist, project_path, topics, cfg, None).await
}

pub async fn answer_recall_with(
    persist: &PersistHandle,
    project_path: &str,
    topics: &[String],
    cfg: &RecallInjectorConfig,
    learning: Option<NearMissLearning>,
) -> RecallAnswer {
    if project_path.is_empty() || topics.is_empty() {
        return RecallAnswer::default();
    }
    let read = topic_memory::expand(project_path, topics);
    let per_topic_limit = i64::try_from(cfg.max_lessons_per_topic).unwrap_or(i64::MAX);
    let Some(rows) = list_rows(persist, project_path, &read, per_topic_limit).await else {
        return RecallAnswer::default();
    };
    if let Some(learning) = learning {
        let missing = topic_memory::unasked(project_path, &unmatched(topics, &rows));
        spawn_learning(persist.clone(), project_path.to_owned(), missing, learning);
    }
    let rows = order_by_requested_topics(&read, rows);
    match formatter::format_recall_reminder(&read, &rows) {
        Some(text) => RecallAnswer {
            lesson_ids: rows.iter().map(|row| row.id).collect(),
            text: Some(text),
        },
        None => RecallAnswer::default(),
    }
}

fn unmatched(typed: &[String], rows: &[LessonRow]) -> Vec<String> {
    typed
        .iter()
        .filter(|topic| {
            !rows
                .iter()
                .any(|r| r.topic.as_deref() == Some(topic.as_str()))
        })
        .cloned()
        .collect()
}

fn spawn_learning(
    persist: PersistHandle,
    project_path: String,
    mut missing: Vec<String>,
    learning: NearMissLearning,
) {
    missing.truncate(topic::MAX_TYPED);
    if missing.is_empty() {
        return;
    }
    topic_memory::mark_asked(&project_path, &missing);
    tokio::spawn(async move {
        learn(&persist, &project_path, &missing, &learning).await;
    });
}

async fn learn(
    persist: &PersistHandle,
    project_path: &str,
    missing: &[String],
    learning: &NearMissLearning,
) {
    let session_id = match learning.session {
        Some(uuid) => persist.session_row_id(uuid).await.ok().flatten(),
        None => None,
    };
    let text = missing.join(", ");
    let mappings = topic::map_near_misses(
        Some(&learning.env),
        persist,
        project_path,
        missing,
        NearMissContext {
            text: &text,
            request_id: None,
            session_id,
        },
    )
    .await;
    for mapping in mappings {
        topic_memory::remember(project_path, &mapping.typed, &mapping.mapped);
    }
}

#[cfg(test)]
#[path = "answer_tests.rs"]
mod tests;
