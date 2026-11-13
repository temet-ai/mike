use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::HashSet;
use std::time::Duration;

use serde::Serialize;

use crate::analysis::embeddings::{EmbeddingConfig, EmbeddingService};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{
    DirectiveSource, EmbedderRow, NewItem, SemanticWindow, TurnSource,
};

use super::check::embed_checked;
use super::config::SemanticConfig;
use super::facets::{ending_text, prompt_head};
use super::{BUILDER_VERSION, SemanticError};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct BuildReport {
    pub turns: usize,
    pub copied: usize,
    pub embedded: u64,
    pub directives: u64,
    pub last_turn: i64,
    pub egress_blocked: bool,
}

pub struct Builder<'a> {
    pub persist: &'a PersistHandle,
    pub embedding: &'a EmbeddingService,
    pub config: &'a SemanticConfig,
    pub embedder: &'a EmbedderRow,
}

struct Planned {
    item: NewItem,
    text: String,
}

fn turn_item(t: &TurnSource, kind: &'static str, text: String, embedder: &EmbedderRow) -> Planned {
    let effective_at = if kind == "turn_ending" {
        t.closed_at.unwrap_or(t.created_at)
    } else {
        t.created_at
    };
    Planned {
        item: NewItem {
            kind,
            session_id: Some(t.session_id),
            operator_turn_id: Some(t.turn_id),
            project_id: t.project_id,
            request_id: Some(t.request_id),
            source_ref: format!("memory.operator_turns:{}", t.turn_id),
            text: text.clone(),
            embedder_key: embedder.embedder_key.clone(),
            vector_source: "embedded",
            builder_version: BUILDER_VERSION,
            effective_at,
            data: serde_json::json!({}),
            ..NewItem::default()
        },
        text,
    }
}

fn plan_turns(turns: &[TurnSource], copied: &HashSet<i64>, b: &Builder<'_>) -> Vec<Planned> {
    let chunk = usize::try_from(b.embedder.chunk_max_chars).unwrap_or(1024);
    let mut out = Vec::new();
    for t in turns {
        if !copied.contains(&t.turn_id)
            && let Some(head) = prompt_head(&t.user_prompt, chunk)
        {
            out.push(turn_item(t, "turn_prompt", head, b.embedder));
        }
        let ending = ending_text(&t.final_text, &t.text_spans, b.config.ending_chars);
        if !ending.is_empty() {
            out.push(turn_item(t, "turn_ending", ending, b.embedder));
        }
    }
    out
}

fn same_space(snapshot: &EmbeddingConfig, embedder: &EmbedderRow) -> bool {
    snapshot.model == embedder.model
        && i32::try_from(snapshot.chunk_max_chars).ok() == Some(embedder.chunk_max_chars)
}

impl Builder<'_> {
    async fn embed_and_insert(&self, planned: Vec<Planned>) -> Result<u64, SemanticError> {
        if planned.is_empty() {
            return Ok(0);
        }
        let texts: Vec<String> = planned.iter().map(|p| p.text.clone()).collect();
        let (snapshot, vectors) =
            embed_checked(self.embedding, &texts, self.config.allow_remote_embedding).await?;
        if !same_space(&snapshot, self.embedder) {
            return Err(SemanticError::SnapshotChanged(format!(
                "{} / {} chars while building for {}",
                snapshot.model, snapshot.chunk_max_chars, self.embedder.embedder_key
            )));
        }
        let items: Vec<NewItem> = planned
            .into_iter()
            .zip(vectors)
            .map(|(p, embedding)| NewItem {
                embedding,
                ..p.item
            })
            .collect();
        Ok(self
            .persist
            .knowledge()
            .insert_semantic_items(&items)
            .await?)
    }

    async fn copy_prompts(&self, turns: &[TurnSource]) -> Result<HashSet<i64>, SemanticError> {
        let Some(from) = self.embedder.copy_valid_from else {
            return Ok(HashSet::new());
        };
        let ids: Vec<i64> = turns.iter().map(|t| t.turn_id).collect();
        let copied = self
            .persist
            .knowledge()
            .copy_semantic_prompt_items(
                &ids,
                &self.embedder.embedder_key,
                BUILDER_VERSION,
                &self.embedder.model,
                from,
            )
            .await?;
        Ok(copied.into_iter().collect())
    }

    pub async fn build_turns(&self, turns: &[TurnSource]) -> Result<BuildReport, SemanticError> {
        let copied = self.copy_prompts(turns).await?;
        let mut report = BuildReport {
            turns: turns.len(),
            copied: copied.len(),
            last_turn: turns.last().map_or(0, |t| t.turn_id),
            ..BuildReport::default()
        };
        match self
            .embed_and_insert(plan_turns(turns, &copied, self))
            .await
        {
            Ok(n) => report.embedded = n,
            Err(SemanticError::EgressBlocked) => report.egress_blocked = true,
            Err(e) => return Err(e),
        }
        Ok(report)
    }

    pub async fn build_batch(&self, window: &SemanticWindow) -> Result<BuildReport, SemanticError> {
        let turns = self
            .persist
            .knowledge()
            .turns_needing_semantic_items(&self.embedder.embedder_key, BUILDER_VERSION, window)
            .await?;
        self.build_turns(&turns).await
    }

    pub async fn build_directives(&self, limit: i64) -> Result<u64, SemanticError> {
        let rows = self
            .persist
            .knowledge()
            .directives_needing_semantic_items(&self.embedder.embedder_key, BUILDER_VERSION, limit)
            .await?;
        let planned: Vec<Planned> = rows
            .iter()
            .filter_map(|d| directive_item(d, self.embedder))
            .collect();
        match self.embed_and_insert(planned).await {
            Ok(n) => Ok(n),
            Err(SemanticError::EgressBlocked) => Ok(0),
            Err(e) => Err(e),
        }
    }

    pub async fn ensure_turn_items(&self, turn_id: i64) -> Result<BuildReport, SemanticError> {
        let Some(turn) = self
            .persist
            .knowledge()
            .semantic_turn_source(turn_id)
            .await?
        else {
            return Ok(BuildReport::default());
        };
        let timeout = Duration::from_millis(self.config.inline_timeout_ms);
        match tokio::time::timeout(timeout, self.build_turns(std::slice::from_ref(&turn))).await {
            Ok(result) => result,
            Err(_) => Err(SemanticError::Timeout(self.config.inline_timeout_ms)),
        }
    }
}

fn span_of(data: &serde_json::Value) -> Option<(usize, usize)> {
    let span = data.get("span")?.as_array()?;
    let start = usize::try_from(span.first()?.as_i64()?).ok()?;
    let end = usize::try_from(span.get(1)?.as_i64()?).ok()?;
    (end > start).then_some((start, end))
}

fn directive_item(d: &DirectiveSource, embedder: &EmbedderRow) -> Option<Planned> {
    let (start, end) = span_of(&d.data)?;
    let text: String = d.prompt.chars().skip(start).take(end - start).collect();
    if text.trim().is_empty() {
        return None;
    }
    Some(Planned {
        item: NewItem {
            kind: "directive",
            session_id: Some(d.session_id),
            operator_turn_id: d.operator_turn_id,
            project_id: d.project_id,
            request_id: Some(d.request_id),
            source_ref: format!("memory.dossier_entries:{}", d.entry_id),
            text: text.clone(),
            embedder_key: embedder.embedder_key.clone(),
            vector_source: "embedded",
            builder_version: BUILDER_VERSION,
            effective_at: d.effective_at,
            data: serde_json::json!({ "class": d.data.get("class"), "polarity": d.data.get("polarity") }),
            ..NewItem::default()
        },
        text,
    })
}
