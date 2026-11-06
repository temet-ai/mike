use serde::Serialize;

use crate::settings::meta_semantic as keys;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalScopeSetting {
    Operator,
    Project,
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelapseEvidence {
    Off,
    Record,
    Input,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemanticConfig {
    pub allow_remote_embedding: bool,
    pub sweep_interval_secs: u64,
    pub batch_turns: i64,
    pub ending_chars: usize,
    pub inline_timeout_ms: u64,
    pub floor_quantile: f64,
    pub floor_sample_turns: i64,
    pub retrieval_scope: RetrievalScopeSetting,
    pub retrieval_k: usize,
    pub relapse_evidence: RelapseEvidence,
    pub tagger_min_score: f64,
    pub cluster_floor_quantile: f64,
    pub cluster_min_members: usize,
    pub cluster_min_sessions: usize,
    pub drift_z: f64,
    pub drift_min_current: i64,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            allow_remote_embedding: false,
            sweep_interval_secs: 300,
            batch_turns: 64,
            ending_chars: 1_024,
            inline_timeout_ms: 5_000,
            floor_quantile: 0.999,
            floor_sample_turns: 300,
            retrieval_scope: RetrievalScopeSetting::Operator,
            retrieval_k: 3,
            relapse_evidence: RelapseEvidence::Record,
            tagger_min_score: 0.05,
            cluster_floor_quantile: 0.99,
            cluster_min_members: 4,
            cluster_min_sessions: 3,
            drift_z: 3.0,
            drift_min_current: 30,
        }
    }
}

fn int<T: TryFrom<i64>>(rows: &[(String, serde_json::Value)], key: &str, default: T) -> T {
    rows.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_i64())
        .and_then(|n| T::try_from(n).ok())
        .unwrap_or(default)
}

fn float(rows: &[(String, serde_json::Value)], key: &str, default: f64) -> f64 {
    rows.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_f64())
        .unwrap_or(default)
}

fn flag(rows: &[(String, serde_json::Value)], key: &str, default: bool) -> bool {
    rows.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_bool())
        .unwrap_or(default)
}

fn text<'a>(rows: &'a [(String, serde_json::Value)], key: &str) -> Option<&'a str> {
    rows.iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_str())
}

impl SemanticConfig {
    pub fn from_rows(rows: &[(String, serde_json::Value)]) -> Self {
        let d = Self::default();
        let retrieval_scope = match text(rows, keys::RETRIEVAL_SCOPE) {
            Some("project") => RetrievalScopeSetting::Project,
            Some("session") => RetrievalScopeSetting::Session,
            _ => d.retrieval_scope,
        };
        let relapse_evidence = match text(rows, keys::RELAPSE_EVIDENCE) {
            Some("off") => RelapseEvidence::Off,
            Some("input") => RelapseEvidence::Input,
            _ => d.relapse_evidence,
        };
        Self {
            allow_remote_embedding: flag(
                rows,
                keys::ALLOW_REMOTE_EMBEDDING,
                d.allow_remote_embedding,
            ),
            sweep_interval_secs: int(rows, keys::SWEEP_INTERVAL_SECS, d.sweep_interval_secs),
            batch_turns: int(rows, keys::BATCH_TURNS, d.batch_turns),
            ending_chars: int(rows, keys::ENDING_CHARS, d.ending_chars),
            inline_timeout_ms: int(rows, keys::INLINE_TIMEOUT_MS, d.inline_timeout_ms),
            floor_quantile: float(rows, keys::FLOOR_QUANTILE, d.floor_quantile),
            floor_sample_turns: int(rows, keys::FLOOR_SAMPLE_TURNS, d.floor_sample_turns),
            retrieval_scope,
            retrieval_k: int(rows, keys::RETRIEVAL_K, d.retrieval_k),
            relapse_evidence,
            tagger_min_score: float(rows, keys::TAGGER_MIN_SCORE, d.tagger_min_score),
            cluster_floor_quantile: float(
                rows,
                keys::CLUSTER_FLOOR_QUANTILE,
                d.cluster_floor_quantile,
            ),
            cluster_min_members: int(rows, keys::CLUSTER_MIN_MEMBERS, d.cluster_min_members),
            cluster_min_sessions: int(rows, keys::CLUSTER_MIN_SESSIONS, d.cluster_min_sessions),
            drift_z: float(rows, keys::DRIFT_Z, d.drift_z),
            drift_min_current: int(rows, keys::DRIFT_MIN_CURRENT, d.drift_min_current),
        }
    }
}
