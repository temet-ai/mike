use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::analysis::embeddings::EmbeddingService;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{EmbedderRow, NewItem};

use super::check::embed_checked;
use super::{BUILDER_VERSION, SemanticError};

const EMBED_BATCH: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Instance {
    pub pattern: String,
    pub turn_id: i64,
    pub session_uuid: String,
    pub operator_quote: String,
    pub assistant_behaviour: Option<String>,
    pub correcting_request_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportPlan {
    pub file_sha256: String,
    pub instances: Vec<Instance>,
    pub patterns: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportReport {
    pub instances: usize,
    pub patterns: usize,
    pub without_correcting_request: Vec<(String, i64)>,
    pub written: u64,
    pub dry_run: bool,
}

fn read(path: &Path) -> Result<String, SemanticError> {
    std::fs::read_to_string(path)
        .map_err(|e| SemanticError::Import(format!("{}: {e}", path.display())))
}

fn next_genuine_requests(turns_jsonl: &str) -> HashMap<i64, i64> {
    turns_jsonl
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|v| {
            Some((
                v.get("turn_id")?.as_i64()?,
                v.get("next_genuine_request_id")?.as_i64()?,
            ))
        })
        .collect()
}

fn instance(pattern: &str, c: &Value, next: &HashMap<i64, i64>) -> Option<Instance> {
    let turn_id = c.get("turn_id")?.as_i64()?;
    Some(Instance {
        pattern: pattern.to_owned(),
        turn_id,
        session_uuid: c.get("session_uuid")?.as_str()?.to_owned(),
        operator_quote: c
            .get("operator_quote")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        assistant_behaviour: c
            .get("assistant_behaviour")
            .and_then(Value::as_str)
            .map(str::to_owned),
        correcting_request_id: next.get(&turn_id).copied(),
    })
}

pub fn parse_mirror(mirror_json: &str, turns_jsonl: &str) -> Result<ImportPlan, SemanticError> {
    let doc: Value = serde_json::from_str(mirror_json)
        .map_err(|e| SemanticError::Import(format!("mirror.json: {e}")))?;
    let next = next_genuine_requests(turns_jsonl);
    let results = doc
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut instances = Vec::new();
    for r in results
        .iter()
        .filter(|r| r.get("survives").and_then(Value::as_bool) == Some(true))
    {
        let Some(pattern) = r.pointer("/p/name").and_then(Value::as_str) else {
            continue;
        };
        let confirmed = r
            .pointer("/audit/confirmed")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        instances.extend(confirmed.iter().filter_map(|c| instance(pattern, c, &next)));
    }
    let patterns = instances
        .iter()
        .map(|i| i.pattern.clone())
        .collect::<BTreeSet<_>>()
        .len();
    Ok(ImportPlan {
        file_sha256: hex::encode(Sha256::digest(mirror_json.as_bytes())),
        instances,
        patterns,
    })
}

pub fn load_plan(dir: &Path) -> Result<ImportPlan, SemanticError> {
    parse_mirror(
        &read(&dir.join("mirror.json"))?,
        &read(&dir.join("turns.jsonl"))?,
    )
}

fn anchor(i: &Instance, text: (String, bool), embedder: &EmbedderRow, sha: &str) -> NewItem {
    let (text, from_request) = text;
    NewItem {
        kind: "anchor",
        request_id: if from_request {
            i.correcting_request_id
        } else {
            None
        },
        origin_session_uuid: Some(i.session_uuid.clone()),
        origin_turn_id: Some(i.turn_id),
        source_ref: format!("mirror:{}:{}", i.pattern, i.turn_id),
        pattern: Some(i.pattern.clone()),
        text,
        embedder_key: embedder.embedder_key.clone(),
        vector_source: "embedded",
        builder_version: BUILDER_VERSION,
        effective_at: Utc::now(),
        data: serde_json::json!({
            "text_source": if from_request { "request" } else { "operator_quote" },
            "assistant_behaviour": i.assistant_behaviour,
            "import": { "file_sha256": sha },
        }),
        ..NewItem::default()
    }
}

pub struct AnchorImport<'a> {
    pub persist: &'a PersistHandle,
    pub embedding: &'a EmbeddingService,
    pub embedder: &'a EmbedderRow,
    pub allow_remote: bool,
}

impl AnchorImport<'_> {
    async fn texts(&self, plan: &ImportPlan) -> Result<Vec<(String, bool)>, SemanticError> {
        let ids: Vec<i64> = plan
            .instances
            .iter()
            .filter_map(|i| i.correcting_request_id)
            .collect();
        let prompts: HashMap<i64, String> = self
            .persist
            .semantic_request_prompts(&ids)
            .await?
            .into_iter()
            .collect();
        Ok(plan
            .instances
            .iter()
            .map(
                |i| match i.correcting_request_id.and_then(|id| prompts.get(&id)) {
                    Some(p) => (p.clone(), true),
                    None => (i.operator_quote.clone(), false),
                },
            )
            .collect())
    }

    pub async fn run(
        &self,
        plan: &ImportPlan,
        execute: bool,
    ) -> Result<ImportReport, SemanticError> {
        let texts = self.texts(plan).await?;
        let without: Vec<(String, i64)> = plan
            .instances
            .iter()
            .zip(&texts)
            .filter(|(_, (_, from_request))| !from_request)
            .map(|(i, _)| (i.pattern.clone(), i.turn_id))
            .collect();
        let mut written = 0;
        if execute {
            let pairs: Vec<(&Instance, &(String, bool))> = plan
                .instances
                .iter()
                .zip(&texts)
                .filter(|(_, (t, _))| !t.trim().is_empty())
                .collect();
            for chunk in pairs.chunks(EMBED_BATCH) {
                let batch: Vec<String> = chunk.iter().map(|(_, (t, _))| t.clone()).collect();
                let (_, vectors) = embed_checked(self.embedding, &batch, self.allow_remote).await?;
                let items: Vec<NewItem> = chunk
                    .iter()
                    .zip(vectors)
                    .map(|((i, t), v)| NewItem {
                        embedding: v,
                        ..anchor(i, (*t).clone(), self.embedder, &plan.file_sha256)
                    })
                    .collect();
                written += self.persist.insert_semantic_items(&items).await?;
            }
        }
        Ok(ImportReport {
            instances: plan.instances.len(),
            patterns: plan.patterns,
            without_correcting_request: without,
            written,
            dry_run: !execute,
        })
    }
}
