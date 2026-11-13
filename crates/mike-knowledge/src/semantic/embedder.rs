use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::analysis::embeddings::EmbeddingConfig;

pub const CANARY_VERSION: &str = "canary-v1";
pub const PREFIX_VERSION: &str = "doc-prefix-v1";
pub const SAME_EMBEDDER_MIN_COSINE: f64 = 0.9999;
pub const VERIFY_SAMPLE: i64 = 200;

pub const CANARY_V1: [&str; 8] = [
    "continue",
    "commit and push",
    "why did you stop?",
    "You said the retry was added and tested, but the test file you edited is not the one the gate runs. \
     Run the real suite, show me the output, and only then tell me it is done. If the suite is red, fix the \
     code rather than the test, and do not touch the lint thresholds to make it pass. I would rather have a \
     red gate and an honest report than a green gate that hides the failure.",
    "Before you change the schema, list every query that reads memory.operator_turns.outcome_id, including \
     the views and the MCP tools, and tell me which of them would read a different value after your change. \
     I want the blast radius first, then the migration, then the code, and a measurement of the slowest \
     query before and after. Do not start a worktree for this; use the warm checkout.",
    "The dashboard shows the session as idle but the agent is still waiting on a background build that it \
     launched twenty minutes ago. Check whether the work registry has the build as a running item, whether \
     anything will wake the session when it finishes, and if nothing will, say so plainly instead of promising \
     to report back. Waiting without a wake is the stall we keep hitting.",
    "Read the operator's messages from last week about the fleet coordinator and write down, in his own words, \
     what he asked for and what he corrected. Quote each message whole with its date. Do not summarise, do not \
     paraphrase, and do not add anything he did not say. If two messages disagree, list both and say which one \
     came later, because the later one is the standing instruction.",
    "The steerer fired three times on a turn that was already correct, and each nudge cost a call and an \
     interruption. Pull the evaluation rows for that session, show me the question each one answered and the \
     evidence it cited, and tell me whether the gate should have stayed closed. If the rule is wrong, propose \
     the smallest change and the replay that would prove it, without shipping anything yet.",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanaryVectors {
    pub version: String,
    pub texts: Vec<String>,
    pub vectors: Vec<Vec<f32>>,
}

impl CanaryVectors {
    pub fn new(vectors: Vec<Vec<f32>>) -> Self {
        Self {
            version: CANARY_VERSION.to_owned(),
            texts: CANARY_V1.iter().map(|t| (*t).to_owned()).collect(),
            vectors,
        }
    }
}

pub fn canary_texts() -> Vec<String> {
    CANARY_V1.iter().map(|t| (*t).to_owned()).collect()
}

pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (f64::from(*x), f64::from(*y));
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

pub fn embedder_key(
    model: &str,
    dim: usize,
    chunk_max_chars: usize,
    since: DateTime<Utc>,
) -> String {
    format!(
        "{model}|{dim}|{chunk_max_chars}|{PREFIX_VERSION}|since:{}",
        since.to_rfc3339_opts(SecondsFormat::Secs, true)
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbedderShape<'a> {
    pub model: &'a str,
    pub dim: usize,
    pub chunk_max_chars: usize,
    pub prefix_version: &'a str,
    pub canaries: &'a CanaryVectors,
}

pub fn same_embedder(stored: &EmbedderShape<'_>, seen: &EmbedderShape<'_>) -> bool {
    stored.model == seen.model
        && stored.dim == seen.dim
        && stored.chunk_max_chars == seen.chunk_max_chars
        && stored.prefix_version == seen.prefix_version
        && stored.canaries.version == seen.canaries.version
        && stored.canaries.vectors.len() == seen.canaries.vectors.len()
        && stored
            .canaries
            .vectors
            .iter()
            .zip(&seen.canaries.vectors)
            .all(|(a, b)| cosine(a, b) >= SAME_EMBEDDER_MIN_COSINE)
}

pub fn snapshot_differs(a: &EmbeddingConfig, b: &EmbeddingConfig) -> bool {
    a.model != b.model || a.url != b.url || a.chunk_max_chars != b.chunk_max_chars
}

pub fn verify_passes(cosines: &[f64]) -> bool {
    !cosines.is_empty() && cosines.iter().all(|c| *c >= SAME_EMBEDDER_MIN_COSINE)
}
