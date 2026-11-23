use crate::web_base::capability::{CapabilityRow, CapabilityState};
use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use std::sync::{Arc, LazyLock};

const ID: &str = "knowledge_consolidation";
const LABEL: &str = "Knowledge consolidation";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Health {
    pub running: bool,
    pub leader: bool,
    pub last_pass_at: Option<DateTime<Utc>>,
    pub last_summary: String,
    pub error: Option<(DateTime<Utc>, String)>,
}

static HEALTH: LazyLock<ArcSwap<Health>> = LazyLock::new(|| ArcSwap::from_pointee(Health::default()));

pub fn snapshot() -> Health {
    (**HEALTH.load()).clone()
}

fn update(change: impl Fn(&mut Health)) {
    HEALTH.rcu(|current| {
        let mut next = (**current).clone();
        change(&mut next);
        Arc::new(next)
    });
}

pub fn set_running(running: bool) {
    update(|h| h.running = running);
}

pub fn not_leader() {
    update(|h| h.leader = false);
}

pub fn failed(message: &str) {
    let message = message.to_owned();
    update(|h| h.error = Some((Utc::now(), message.clone())));
}

pub fn passed(summary: &str, error: Option<&str>) {
    let summary = summary.to_owned();
    update(|h| {
        h.leader = true;
        h.last_pass_at = Some(Utc::now());
        h.last_summary.clone_from(&summary);
        if let Some(message) = error {
            h.error = Some((Utc::now(), message.to_owned()));
        } else {
            h.error = None;
        }
    });
}

pub fn idle() {
    update(|h| h.leader = true);
}

fn row(state: CapabilityState, detail: String, fix: &str) -> CapabilityRow {
    CapabilityRow {
        id: ID,
        label: LABEL,
        state,
        detail,
        fix: fix.to_owned(),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Waiting {
    pub files: i64,
    pub other_items: i64,
    pub subjects: i64,
}

fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

pub fn waiting_words(waiting: Waiting) -> String {
    let mut parts = Vec::new();
    if waiting.files > 0 {
        parts.push(format!(
            "{} waiting to be split",
            plural(waiting.files, "file", "files")
        ));
    }
    if waiting.other_items > 0 {
        parts.push(format!(
            "{} waiting to be read",
            plural(waiting.other_items, "lesson or session item", "lessons and session items")
        ));
    }
    if waiting.subjects > 0 {
        parts.push(format!(
            "{} waiting to be judged",
            plural(waiting.subjects, "subject", "subjects")
        ));
    }
    if parts.is_empty() {
        "Nothing waiting.".to_owned()
    } else {
        parts.join(" · ")
    }
}

pub fn capability_row(
    persistence: bool,
    enabled: bool,
    health: &Health,
    waiting: Option<Waiting>,
) -> CapabilityRow {
    if !persistence {
        return row(
            CapabilityState::Off,
            "Persistence is off, so there is nothing to consolidate".to_owned(),
            "",
        );
    }
    if !enabled {
        return row(
            CapabilityState::Off,
            "The knowledge_consolidation toggle is off, so lessons, memory files, rules, agents and skills are not compared with each other".to_owned(),
            "Turn on Knowledge consolidation at /settings/workers",
        );
    }
    if !health.running {
        return row(
            CapabilityState::Inert,
            "The toggle is on but the consolidation worker has not started yet; it checks the toggle every 2 seconds".to_owned(),
            "Wait a few seconds and reload; if it stays inert, check the mike log",
        );
    }
    let queue = waiting.map_or_else(String::new, waiting_words);
    if let Some((at, message)) = &health.error {
        return row(
            CapabilityState::Degraded,
            format!("Newest error at {at}: {message}. {queue}"),
            "Check that the embedding service answers and that the side-call models and the consolidation model are reachable at /settings; the worker retries with backoff and recovers by itself",
        );
    }
    if !health.leader {
        return row(
            CapabilityState::Inert,
            "Another machine holds the knowledge_consolidation lease and does the work".to_owned(),
            "",
        );
    }
    let detail = if queue.is_empty() && health.last_pass_at.is_none() {
        "No pass has finished yet.".to_owned()
    } else {
        queue
    };
    row(CapabilityState::Active, detail, "")
}
