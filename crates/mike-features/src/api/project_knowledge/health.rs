use crate::embed_worker::knowledge_report::{KnowledgeSweepReport, fresh_report};
use crate::semantic::KnowledgeState;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::{KNOWLEDGE_COUNT_CAP, KnowledgeStateCounts};
use crate::web_base::capability::{CapabilityRow, CapabilityState};

pub async fn embedding_capability(state: &KnowledgeState) -> CapabilityRow {
    let service = state.embedding_service();
    let mut row = CapabilityRow {
        id: "project_knowledge",
        label: "Project knowledge embeddings",
        state: CapabilityState::Off,
        detail: "Lexical project search is available; embeddings are disabled".into(),
        fix: "Enable embedding.enabled at /settings/embedding".into(),
    };
    if !service.is_available() {
        return row;
    }
    row.state = CapabilityState::Inert;
    row.fix = "Check persistence and the embedding service. Transient failures keep retrying with backoff; permanent failures stop after ten attempts. Correct the source or embedding configuration to start a new generation.".into();
    let Some(handle) = state.persist() else {
        row.detail = "Knowledge embeddings require persistence".into();
        return row;
    };
    let keys = crate::embed_worker::knowledge::model_keys(&service);
    let model = keys.current().to_owned();
    match handle
        .knowledge()
        .knowledge_embedding_state_counts(&keys)
        .await
    {
        Ok(counts) => {
            let sweep = fresh_report(&model, chrono::Utc::now());
            apply_coverage(&mut row, counts, sweep.as_deref());
        }
        Err(error) => {
            row.detail = format!("Knowledge embedding state unavailable: {error}");
        }
    }
    row
}

pub fn capped_label(count: i64) -> String {
    if count > KNOWLEDGE_COUNT_CAP {
        format!("{KNOWLEDGE_COUNT_CAP}+")
    } else {
        count.to_string()
    }
}

fn coverage_detail(counts: KnowledgeStateCounts, sweep: Option<&KnowledgeSweepReport>) -> String {
    let ready = capped_label(counts.ready);
    let failed = capped_label(counts.failed);
    let current = capped_label(counts.current_version);
    match sweep {
        Some(report) => format!(
            "{ready} sources embedded, {} pending, {failed} with recorded failures, {current} embedded with the current header form; lexical retrieval remains available",
            report.pending_label()
        ),
        None => format!(
            "{ready} sources embedded, {failed} with recorded failures, {current} embedded with the current header form; the pending count comes from the knowledge sweep, which has not reported on this machine recently; lexical retrieval remains available"
        ),
    }
}

pub fn apply_coverage(
    row: &mut CapabilityRow,
    counts: KnowledgeStateCounts,
    sweep: Option<&KnowledgeSweepReport>,
) {
    let pending = sweep.is_some_and(KnowledgeSweepReport::has_pending);
    row.detail = coverage_detail(counts, sweep);
    row.state = if counts.failed > 0 || pending {
        CapabilityState::Degraded
    } else if counts.ready > 0 {
        CapabilityState::Active
    } else {
        CapabilityState::Inert
    };
    if counts.ready > 0 && !pending && counts.failed == 0 {
        row.fix.clear();
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
