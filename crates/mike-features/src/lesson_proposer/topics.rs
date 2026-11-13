use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::collections::HashSet;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::llm::OpenRouterClient;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::topics::UntopicedLesson;

#[cfg(test)]
#[path = "topics_tests.rs"]
pub mod tests;

pub const BATCH: i64 = 30;
const MAX_TOPIC_CHARS: usize = 40;
const KIND_WORDS: [&str; 4] = ["warning", "correction", "convention", "preference"];

pub const TOPIC_PROMPT: &str = "You assign each lesson a topic. A topic is a short kebab-case \
name, two to four words, for the area of work the lesson concerns, so that lessons about the \
same work share one topic and a session working on that area can load them together.\n\n\
Prefer a topic from the project's existing list whenever it fits; coin a new one only when \
none does. A topic names the work (deploying-the-proxy, database-migrations, fleet-seat-briefs), \
never the kind of lesson and never the project's name.\n\n\
Return JSON matching the schema: exactly one assignment per lesson id given, no other ids.";

#[derive(Debug, Deserialize)]
pub struct Assignment {
    pub id: i64,
    pub topic: String,
}

#[derive(Debug, Deserialize)]
pub struct Assignments {
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct TopicPassReport {
    pub project_id: Option<i64>,
    pub batch: usize,
    pub assigned: u64,
    pub error: Option<String>,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "assignments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "integer"},
                        "topic": {"type": "string"}
                    },
                    "required": ["id", "topic"]
                }
            }
        },
        "required": ["assignments"]
    })
}

pub fn normalise_topic(raw: &str) -> Option<String> {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in raw.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch);
        } else {
            pending_dash = true;
        }
    }
    let truncated: String = out.chars().take(MAX_TOPIC_CHARS).collect();
    let topic = truncated.trim_end_matches('-').to_owned();
    if topic.is_empty() || KIND_WORDS.contains(&topic.as_str()) {
        return None;
    }
    Some(topic)
}

pub fn build_input(existing: &[String], batch: &[UntopicedLesson]) -> String {
    let mut out = String::from("## EXISTING TOPICS\n");
    if existing.is_empty() {
        out.push_str("(none yet)\n");
    }
    for topic in existing {
        let _ = writeln!(out, "- {topic}");
    }
    out.push_str("\n## LESSONS\n");
    for lesson in batch {
        let _ = writeln!(out, "[{}] {}\n", lesson.id, lesson.text.trim());
    }
    out
}

pub fn accepted(batch: &[UntopicedLesson], answer: Assignments) -> (Vec<i64>, Vec<String>) {
    let wanted: HashSet<i64> = batch.iter().map(|l| l.id).collect();
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    let mut topics = Vec::new();
    for a in answer.assignments {
        if !wanted.contains(&a.id) || !seen.insert(a.id) {
            continue;
        }
        if let Some(topic) = normalise_topic(&a.topic) {
            ids.push(a.id);
            topics.push(topic);
        }
    }
    (ids, topics)
}

pub async fn run_pass(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    limit: i64,
) -> TopicPassReport {
    let mut report = TopicPassReport::default();
    let batch = match persist.lessons().untopiced_lesson_batch(limit).await {
        Ok(batch) => batch,
        Err(e) => return failed(report, format!("batch: {e}")),
    };
    let Some(first) = batch.first() else {
        return report;
    };
    report.project_id = Some(first.project_id);
    report.batch = batch.len();
    let existing = match persist.lessons().project_topics(first.project_id).await {
        Ok(existing) => existing,
        Err(e) => return failed(report, format!("topics: {e}")),
    };
    let answer = match llm
        .verdict::<Assignments>(TOPIC_PROMPT, &build_input(&existing, &batch), schema())
        .await
    {
        Ok(answer) => answer,
        Err(e) => return failed(report, format!("model: {e}")),
    };
    let (ids, topics) = accepted(&batch, answer);
    match persist.lessons().set_lesson_topics(&ids, &topics).await {
        Ok(n) => report.assigned = n,
        Err(e) => return failed(report, format!("write: {e}")),
    }
    tracing::info!(
        project_id = first.project_id,
        batch = report.batch,
        assigned = report.assigned,
        "lesson topics: pass complete"
    );
    report
}

fn failed(mut report: TopicPassReport, error: String) -> TopicPassReport {
    tracing::warn!(error = %error, "lesson topics: pass failed");
    report.error = Some(error);
    report
}
