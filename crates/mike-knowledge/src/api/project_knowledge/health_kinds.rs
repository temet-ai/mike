use crate::store::persistence::project_knowledge::KnowledgeKindRow;
use crate::web_base::capability::{CapabilityRow, CapabilityState};

const KIND_REMEDY: &str = "Open /settings/knowledge; fix the source or switch the kind off.";

fn needs_attention(kind: &KnowledgeKindRow) -> bool {
    kind.enabled && (kind.failed > 0 || kind.behind)
}

fn describe(kind: &KnowledgeKindRow) -> String {
    let mut parts = vec![format!(
        "{} ({} items, {} embedded, {} waiting, {} failed",
        kind.kind, kind.items, kind.embedded, kind.waiting, kind.failed
    )];
    if let Some(error) = &kind.newest_error {
        parts.push(format!("; newest error: {error}"));
    }
    if let Some(since) = kind.oldest_waiting_ready_at.filter(|_| kind.behind) {
        parts.push(format!("; oldest waiting since {}", since.to_rfc3339()));
    }
    parts.push(")".to_owned());
    parts.concat()
}

pub fn apply_kind_problems(row: &mut CapabilityRow, kinds: &[KnowledgeKindRow]) {
    let problems: Vec<String> = kinds
        .iter()
        .filter(|kind| needs_attention(kind))
        .map(describe)
        .collect();
    if problems.is_empty() {
        return;
    }
    row.state = CapabilityState::Degraded;
    row.detail = format!(
        "{}; knowledge kinds failing or behind: {}",
        row.detail,
        problems.join(", ")
    );
    row.fix = if row.fix.is_empty() {
        KIND_REMEDY.to_owned()
    } else {
        format!("{} {KIND_REMEDY}", row.fix)
    };
}

#[cfg(test)]
#[path = "health_kinds_tests.rs"]
mod tests;
