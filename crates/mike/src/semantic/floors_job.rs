use serde::Serialize;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{
    EmbedderRow, FloorRow, FloorSample, FloorStats, NewFloor,
};

use super::SemanticError;
use super::config::SemanticConfig;
use super::floors::{
    FLOOR_BUILDER_VERSION, Floor, FloorSet, MAX_PAIRS, all_pairs, floor_status, promotion_refused,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FloorLine {
    pub pair: String,
    pub stats: FloorStats,
    pub status: String,
    pub id: Option<i64>,
}

pub async fn floor_set(
    persist: &PersistHandle,
    key: &str,
    quantile: f64,
) -> Result<FloorSet, SemanticError> {
    let rows = persist.semantic_floors_in_force(key, quantile).await?;
    Ok(FloorSet {
        by_pair: rows
            .into_iter()
            .map(|r| {
                (
                    r.pair,
                    Floor {
                        id: r.id,
                        floor: r.floor,
                    },
                )
            })
            .collect(),
    })
}

pub struct FloorRun<'a> {
    pub persist: &'a PersistHandle,
    pub embedder: &'a EmbedderRow,
    pub config: &'a SemanticConfig,
    pub seed: &'a str,
    pub execute: bool,
}

impl FloorRun<'_> {
    async fn one(&self, pair: &str, first_for_key: bool) -> Result<FloorLine, SemanticError> {
        let stats = self
            .persist
            .compute_semantic_floor(&FloorSample {
                embedder_key: &self.embedder.embedder_key,
                pair,
                seed: self.seed,
                sample_turns: self.config.floor_sample_turns,
                max_pairs: MAX_PAIRS,
                quantile: self.config.floor_quantile,
                model: &self.embedder.model,
            })
            .await?;
        let status = floor_status(stats.n_pairs, first_for_key);
        let id = if self.execute {
            let sample = serde_json::json!({
                "seed": self.seed,
                "sample_turns": self.config.floor_sample_turns,
                "max_pairs": MAX_PAIRS,
                "window_days": 60,
                "corr_tiers": ["gold", "relayed", "seed", "live_verdict"],
            });
            Some(
                self.persist
                    .insert_semantic_floor(&NewFloor {
                        embedder_key: &self.embedder.embedder_key,
                        pair,
                        quantile: self.config.floor_quantile,
                        stats: &stats,
                        sample,
                        builder_version: FLOOR_BUILDER_VERSION,
                        status,
                        promoted_from_id: None,
                    })
                    .await?,
            )
        } else {
            None
        };
        Ok(FloorLine {
            pair: pair.to_owned(),
            stats,
            status: status.to_owned(),
            id,
        })
    }

    pub async fn compute_all(&self) -> Result<Vec<FloorLine>, SemanticError> {
        let first = !self
            .persist
            .semantic_key_has_floors(&self.embedder.embedder_key)
            .await?;
        let mut lines = Vec::new();
        for pair in all_pairs() {
            lines.push(self.one(pair, first).await?);
        }
        Ok(lines)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum PromoteResult {
    Promoted(i64),
    DryRun(FloorRow),
    Refused(String),
    NotMonitoring,
}

pub struct Promote<'a> {
    pub pair: &'a str,
    pub from_id: i64,
    pub rebundle: bool,
    pub execute: bool,
    pub llm_facing_pairs: &'a [&'a str],
}

pub async fn promote(
    persist: &PersistHandle,
    p: &Promote<'_>,
) -> Result<PromoteResult, SemanticError> {
    let Some(row) = persist.get_semantic_floor(p.from_id).await? else {
        return Ok(PromoteResult::NotMonitoring);
    };
    if row.status != "monitoring" || row.pair != p.pair {
        return Ok(PromoteResult::NotMonitoring);
    }
    if promotion_refused(p.pair, p.llm_facing_pairs, p.rebundle) {
        return Ok(PromoteResult::Refused(format!(
            "pair {} is read by an LLM-facing consumer; pass --rebundle to change its bundle",
            p.pair
        )));
    }
    if !p.execute {
        return Ok(PromoteResult::DryRun(row));
    }
    let stats = FloorStats {
        n_pairs: row.n_pairs,
        p50: Some(row.p50),
        p95: Some(row.p95),
        p99: Some(row.p99),
        p999: Some(row.p999),
        floor: Some(row.floor),
    };
    let id = persist
        .insert_semantic_floor(&NewFloor {
            embedder_key: &row.embedder_key,
            pair: &row.pair,
            quantile: row.quantile,
            stats: &stats,
            sample: row.sample.clone(),
            builder_version: FLOOR_BUILDER_VERSION,
            status: "in_force",
            promoted_from_id: Some(row.id),
        })
        .await?;
    Ok(PromoteResult::Promoted(id))
}
