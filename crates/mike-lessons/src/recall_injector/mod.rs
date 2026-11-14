use crate::store::persistence::lessons_repo::LessonsPersist as _;
#[cfg(test)]
mod floor_tests;
pub mod formatter;
pub mod parser;
#[cfg(test)]
pub mod push_delivery_tests;
pub mod stage;
#[cfg(test)]
mod tests;
pub mod topic;

use bytes::Bytes;
use serde_json::Value;

use crate::lazy_sections::has_thinking_blocks;
use crate::llm::decisions::env::DecisionEnv;
use crate::llm::decisions::point::Decider;
use crate::proxy::remote_control::inject::{PushDeps, try_push_reminder};
use crate::reminders::inject::{InjectBookkeeping, inject_system_reminder};
use crate::reminders::supply_floor::rows::keep_rows;
use crate::reminders::supply_floor::{self, Candidate, Context};
use crate::store::models::{ReminderSource, RequestId, SystemReminder};
use crate::store::persistence::PersistHandle;

pub use crate::store::persistence::lessons::LessonRow;
pub use parser::scan_recall_tags;

static SERVED_BY_CLIENT: std::sync::LazyLock<dashmap::DashSet<String>> =
    std::sync::LazyLock::new(dashmap::DashSet::new);

pub fn mark_served(session_id: &str) {
    SERVED_BY_CLIENT.insert(session_id.to_owned());
}

pub fn is_served(session_id: &str) -> bool {
    SERVED_BY_CLIENT.contains(session_id)
}

pub use crate::lessons_settings::RecallInjectorConfig;

#[derive(Debug)]
pub enum RecallOutcome {
    Declined(&'static str),
    Injected {
        new_body: Bytes,
        topics: Vec<String>,
        lesson_count: usize,
    },
}

pub struct RecallInjectionRequest<'a> {
    pub raw: &'a [u8],
    pub project_path: &'a str,
    pub persist: &'a Option<PersistHandle>,
    pub cfg: &'a RecallInjectorConfig,
    pub request_id: RequestId,
    pub session_id: &'a str,
    pub push_deps: &'a PushDeps,
}

pub async fn apply_recall_injection(req: RecallInjectionRequest<'_>) -> RecallOutcome {
    apply_recall_injection_with(req, None).await
}

pub async fn apply_recall_injection_with(
    req: RecallInjectionRequest<'_>,
    decisions: Option<&DecisionEnv>,
) -> RecallOutcome {
    let RecallInjectionRequest {
        raw,
        project_path,
        persist,
        cfg,
        request_id,
        session_id,
        push_deps,
    } = req;
    if !cfg.enabled {
        return RecallOutcome::Declined("recall_disabled");
    }

    let Ok(value) = serde_json::from_slice::<Value>(raw) else {
        return RecallOutcome::Declined("recall_body_not_json");
    };

    let assistant_text = collect_assistant_text(&value);
    let topics = scan_recall_tags(&assistant_text);
    if topics.is_empty() {
        return RecallOutcome::Declined("recall_no_tags");
    }

    if has_thinking_blocks(&value) {
        tracing::debug!(
            "recall_injector: R1 guard fired — thinking blocks present, skipping injection"
        );
        return RecallOutcome::Declined("recall_thinking_guard");
    }

    let Some(persist) = persist.as_ref() else {
        return RecallOutcome::Declined("recall_no_persist");
    };
    if project_path.is_empty() {
        return RecallOutcome::Declined("recall_no_project");
    }

    let asking = i64::try_from(request_id.as_u64()).ok();
    let near = decisions.map(|env| (env, assistant_text.as_str(), asking));
    let Some(Gathered {
        rows: all_rows,
        topics,
    }) = gather_lessons_for_topics(persist, project_path, &topics, cfg, near).await
    else {
        return RecallOutcome::Declined("recall_no_lessons");
    };
    let floor = SupplyFloor {
        decisions,
        session_id,
        request_id,
        text: &assistant_text,
    };
    let all_rows = floor.keep(all_rows).await;
    if all_rows.is_empty() {
        return RecallOutcome::Declined("recall_lessons_below_floor");
    }

    let Some(reminder_text) = formatter::format_recall_reminder(&topics, &all_rows) else {
        return RecallOutcome::Declined("recall_nothing_to_format");
    };

    let probe = SystemReminder {
        source: ReminderSource::Recall,
        payload: reminder_text.clone(),
    };
    let admission = push_deps.gate.admit(session_id, &probe);
    if admission.suppression_reason().is_some() {
        return RecallOutcome::Declined("recall_gate_suppressed_duplicate_content");
    }
    if admission.may_wake()
        && let Ok(recipient) = session_id.parse::<uuid::Uuid>()
        && try_push_reminder(push_deps, recipient, &probe).await
    {
        return RecallOutcome::Declined("recall_delivered_out_of_band");
    }

    let outcome = inject_recall_reminder(raw, topics, all_rows, reminder_text, request_id);
    if matches!(outcome, RecallOutcome::Injected { .. }) {
        push_deps.gate.record_delivery(session_id, &probe);
    }
    outcome
}

struct SupplyFloor<'a> {
    decisions: Option<&'a DecisionEnv>,
    session_id: &'a str,
    request_id: RequestId,
    text: &'a str,
}

fn row_candidate(row: &LessonRow) -> Candidate<'_> {
    Candidate {
        id: row.id,
        kind: &row.kind,
        text: &row.text,
    }
}

impl SupplyFloor<'_> {
    async fn keep(&self, rows: Vec<LessonRow>) -> Vec<LessonRow> {
        let decider = self
            .decisions
            .and_then(|env| env.decider(supply_floor::POINT))
            .map(Decider::inline);
        let context = Context::for_request(self.session_id, self.request_id, self.text);
        keep_rows(decider.as_ref(), &context, rows, row_candidate).await
    }
}

#[derive(Debug, Default)]
pub struct RecallAnswer {
    pub lesson_ids: Vec<i64>,
    pub text: Option<String>,
}

pub async fn answer_recall(
    persist: &PersistHandle,
    project_path: &str,
    topics: &[String],
    cfg: &RecallInjectorConfig,
) -> RecallAnswer {
    if project_path.is_empty() || topics.is_empty() {
        return RecallAnswer::default();
    }
    let Some(Gathered { rows, topics }) =
        gather_lessons_for_topics(persist, project_path, topics, cfg, None).await
    else {
        return RecallAnswer::default();
    };
    match formatter::format_recall_reminder(&topics, &rows) {
        Some(text) => RecallAnswer {
            lesson_ids: rows.iter().map(|row| row.id).collect(),
            text: Some(text),
        },
        None => RecallAnswer::default(),
    }
}

struct Gathered {
    rows: Vec<LessonRow>,
    topics: Vec<String>,
}

async fn list_rows(
    persist: &PersistHandle,
    project_path: &str,
    topics: &[String],
    per_topic_limit: i64,
) -> Option<Vec<LessonRow>> {
    match persist
        .lessons()
        .list_recall_lessons(project_path, topics, per_topic_limit)
        .await
    {
        Ok(rows) => Some(rows),
        Err(e) => {
            tracing::warn!(
                error = %e,
                project_path,
                "recall_injector: list_recall_lessons failed — skipping injection"
            );
            None
        }
    }
}

async fn gather_lessons_for_topics(
    persist: &PersistHandle,
    project_path: &str,
    topics: &[String],
    cfg: &RecallInjectorConfig,
    near: Option<(&DecisionEnv, &str, Option<i64>)>,
) -> Option<Gathered> {
    let per_topic_limit = i64::try_from(cfg.max_lessons_per_topic).unwrap_or(i64::MAX);
    let mut rows = list_rows(persist, project_path, topics, per_topic_limit).await?;
    let mut requested = topics.to_vec();
    if let Some((env, context, request)) = near {
        let missing: Vec<String> = topics
            .iter()
            .filter(|t| !rows.iter().any(|r| r.topic.as_deref() == Some(t.as_str())))
            .cloned()
            .collect();
        let mapped = topic::map_near_misses(
            Some(env),
            persist,
            project_path,
            &missing,
            (context, request),
        )
        .await
        .into_iter()
        .map(|m| m.mapped)
        .filter(|m| !requested.contains(m))
        .collect::<Vec<_>>();
        if !mapped.is_empty() {
            rows.extend(list_rows(persist, project_path, &mapped, per_topic_limit).await?);
            requested.extend(mapped);
        }
    }
    let all_rows = order_by_requested_topics(&requested, rows);
    (!all_rows.is_empty()).then_some(Gathered {
        rows: all_rows,
        topics: requested,
    })
}

fn order_by_requested_topics(topics: &[String], rows: Vec<LessonRow>) -> Vec<LessonRow> {
    let mut by_topic: std::collections::HashMap<String, Vec<LessonRow>> =
        std::collections::HashMap::new();
    for row in rows {
        if let Some(topic) = row.topic.clone() {
            by_topic.entry(topic).or_default().push(row);
        }
    }
    topics
        .iter()
        .filter_map(|topic| by_topic.remove(topic))
        .flatten()
        .collect()
}

fn inject_recall_reminder(
    raw: &[u8],
    topics: Vec<String>,
    all_rows: Vec<LessonRow>,
    reminder_text: String,
    request_id: RequestId,
) -> RecallOutcome {
    let mut post = Bytes::from(raw.to_vec());
    let mut stored: Option<Bytes> = None;
    let mut applied = false;
    let reminder = SystemReminder {
        source: ReminderSource::Recall,
        payload: reminder_text,
    };
    if !inject_system_reminder(
        &reminder,
        &mut InjectBookkeeping {
            post_filter_body: &mut post,
            filtered_stored: &mut stored,
            filter_applied: &mut applied,
            request_id,
        },
    ) {
        return RecallOutcome::Declined("recall_inject_failed");
    }

    let lesson_count = all_rows.len();

    RecallOutcome::Injected {
        new_body: post,
        topics,
        lesson_count,
    }
}

fn collect_assistant_text(value: &Value) -> String {
    let Some(messages) = value.get("messages").and_then(Value::as_array) else {
        return String::new();
    };

    let mut out = String::new();
    for msg in messages {
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
        if role != "assistant" {
            continue;
        }
        match msg.get("content") {
            Some(Value::String(s)) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(s);
            }
            Some(Value::Array(blocks)) => {
                for block in blocks {
                    if let Some("text") = block.get("type").and_then(Value::as_str)
                        && let Some(text) = block.get("text").and_then(Value::as_str)
                    {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(text);
                    }
                }
            }
            _ => {}
        }
    }
    out
}
