use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Deserialize)]
pub struct GtFinal {
    pub label: String,
    pub delivery: String,
    #[serde(default)]
    pub irreversible_action: bool,
    #[serde(default)]
    pub own_prompt_corrects_prior: bool,
    #[serde(default)]
    pub assistant_self_correction: bool,
    #[serde(default)]
    pub failure_tags: Vec<String>,
    pub severity: Option<String>,
    pub evidence: Option<String>,
    pub confidence: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GtRow {
    pub turn_id: i64,
    #[serde(rename = "final")]
    pub final_label: GtFinal,
    #[serde(default)]
    pub supplement: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroundTruthFile {
    pub rows: Vec<GtRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeFinal {
    pub label: String,
    pub evidence: Option<String>,
    pub confidence: Option<String>,
    pub severity: Option<String>,
    #[serde(default)]
    pub failure_tags: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeRow {
    pub turn_id: i64,
    #[serde(rename = "final")]
    pub final_label: SeFinal,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MappedPatterns {
    pub turn_id: i64,
    pub patterns: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewPattern {
    pub name: String,
    pub definition: String,
    pub turn_ids: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeMapping {
    pub mapped: Vec<MappedPatterns>,
    pub new_patterns: Vec<NewPattern>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionEndsFile {
    pub rows: Vec<SeRow>,
    pub mapping: SeMapping,
    pub kappa_label: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CensusRow {
    pub turn_id: i64,
    pub label: String,
    pub evidence: Option<String>,
    pub confidence: Option<String>,
    pub severity: Option<String>,
    #[serde(default)]
    pub failure_tags: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaggedTurn {
    pub id: i64,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuditInstance {
    pub turn_id: i64,
    pub operator_quote: Option<String>,
    pub assistant_behaviour: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Audit {
    #[serde(default)]
    pub confirmed: Vec<AuditInstance>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatternName {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MirrorResult {
    pub p: PatternName,
    pub audit: Audit,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MirrorFile {
    pub tagged: Vec<TaggedTurn>,
    pub results: Vec<MirrorResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TurnLine {
    pub turn_id: i64,
    pub session_uuid: String,
    pub created_at: String,
    pub project_path: Option<String>,
    pub own_prompt_kind: Option<String>,
    pub stored_outcome: Option<String>,
    pub user_prompt_head: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileManifest {
    pub name: &'static str,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone)]
pub struct Assets {
    pub ground_truth: GroundTruthFile,
    pub session_ends: SessionEndsFile,
    pub census: Vec<CensusRow>,
    pub mirror: MirrorFile,
    pub turns: Vec<TurnLine>,
    pub manifest: Vec<FileManifest>,
}

fn read(
    dir: &Path,
    name: &'static str,
    manifest: &mut Vec<FileManifest>,
) -> anyhow::Result<String> {
    let path = dir.join(name);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    manifest.push(FileManifest {
        name,
        sha256: crate::eval::bundles::sha256_hex(text.as_bytes()),
        bytes: text.len(),
    });
    Ok(text)
}

fn parse_json<T: serde::de::DeserializeOwned>(name: &str, text: &str) -> anyhow::Result<T> {
    serde_json::from_str(text).map_err(|e| anyhow::anyhow!("{name} line {}: {e}", e.line()))
}

fn parse_lines<T: serde::de::DeserializeOwned>(name: &str, text: &str) -> anyhow::Result<Vec<T>> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).map_err(|e| anyhow::anyhow!("{name} line {}: {e}", i + 1))
        })
        .collect()
}

pub fn load_assets(dir: &Path) -> anyhow::Result<Assets> {
    let mut manifest = Vec::new();
    let ground_truth = parse_json(
        "ground_truth.json",
        &read(dir, "ground_truth.json", &mut manifest)?,
    )?;
    let session_ends = parse_json(
        "session-ends.json",
        &read(dir, "session-ends.json", &mut manifest)?,
    )?;
    let census = parse_lines("census.jsonl", &read(dir, "census.jsonl", &mut manifest)?)?;
    let mirror = parse_json("mirror.json", &read(dir, "mirror.json", &mut manifest)?)?;
    let turns = parse_lines("turns.jsonl", &read(dir, "turns.jsonl", &mut manifest)?)?;
    Ok(Assets {
        ground_truth,
        session_ends,
        census,
        mirror,
        turns,
        manifest,
    })
}
