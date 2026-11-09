use std::time::Duration;

use super::ProposerDeps;
use crate::embed_worker::knowledge::{model_key, valid_vector};
use crate::steerer::guard::gates::tail_chars;
use crate::store::persistence::lessons::proposals::CorrectionCandidate;
use crate::store::persistence::project_knowledge::{GuidanceRow, ShownGuidance};

const GUIDANCE_K: i64 = 3;
const QUERY_CHARS: usize = 1_500;
const EMBED_DEADLINE: Duration = Duration::from_secs(3);

fn query_text(candidate: &CorrectionCandidate) -> String {
    let mut text = candidate.correcting_message.clone().unwrap_or_default();
    if !candidate.unmet_ask.trim().is_empty() {
        text.push('\n');
        text.push_str(&candidate.unmet_ask);
    }
    tail_chars(text.trim(), QUERY_CHARS).to_owned()
}

async fn query_vector(deps: &ProposerDeps<'_>, text: &str) -> Option<(String, String, String)> {
    let service = deps.embedding;
    if !service.is_available() {
        return None;
    }
    let key = model_key(service);
    match tokio::time::timeout(EMBED_DEADLINE, service.embed_query(text)).await {
        Ok(Ok(vector)) if valid_vector(&vector, service.dim()) && model_key(service) == key => {
            Some((
                crate::store::operator_turns::vector_to_text_literal(&vector),
                key,
                service.model_name(),
            ))
        }
        _ => None,
    }
}

pub(super) async fn closest(
    deps: &ProposerDeps<'_>,
    candidate: &CorrectionCandidate,
) -> Vec<GuidanceRow> {
    closest_to_text(deps, candidate.project_id, &query_text(candidate)).await
}

pub(super) async fn closest_to_text(
    deps: &ProposerDeps<'_>,
    project_id: i64,
    text: &str,
) -> Vec<GuidanceRow> {
    let text = tail_chars(text.trim(), QUERY_CHARS);
    if text.is_empty() {
        return Vec::new();
    }
    let vector = query_vector(deps, text).await;
    let mut rows = match deps
        .persist
        .closest_guidance(project_id, text, vector.as_ref(), GUIDANCE_K)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, project_id,
                "lesson_proposer: memory and skill lookup failed");
            Vec::new()
        }
    };
    rows.extend(super::file_guidance::closest(deps, project_id, text).await);
    rows
}

pub(super) async fn record(
    deps: &ProposerDeps<'_>,
    candidate: &CorrectionCandidate,
    rows: &[GuidanceRow],
    reason: &str,
) {
    let shown: Vec<ShownGuidance<'_>> = rows
        .iter()
        .map(|row| ShownGuidance {
            row,
            cited: row.cited_by(reason),
        })
        .collect();
    if let Err(error) = deps
        .persist
        .record_proposal_guidance(candidate.verdict_id, candidate.project_id, &shown)
        .await
    {
        tracing::warn!(%error, verdict_id = candidate.verdict_id,
            "lesson_proposer: recording the memory and skill text shown failed");
    }
}
