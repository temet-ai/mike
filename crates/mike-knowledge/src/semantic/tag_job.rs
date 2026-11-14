use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde::Serialize;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{
    AnchorCandidateRow, CorrectionForTagging, EmbedderRow, NewAssignment, NewCandidate,
    parse_vector,
};

use super::SemanticError;
use super::cluster::{CLUSTER_VERSION, GroupRules, Member, mutual_groups};
use super::config::SemanticConfig;
use super::floors_job::floor_set;
use super::tagger::{AnchorHit, CorrectionRef, TAGGER_VERSION, assign};
use crate::llm::decisions::env::DecisionEnv;

const BATCH: i64 = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TagReport {
    pub corrections: usize,
    pub assigned: usize,
    pub written: u64,
    pub no_floor: bool,
}

fn hits_for(rows: &[AnchorCandidateRow], floor: f64) -> HashMap<i64, Vec<AnchorHit>> {
    let mut out: HashMap<i64, Vec<AnchorHit>> = HashMap::new();
    for r in rows {
        out.entry(r.correction_item_id)
            .or_default()
            .push(AnchorHit {
                item_id: r.anchor_item_id,
                pattern: r.pattern.clone().unwrap_or_default(),
                origin_session_uuid: r.origin_session_uuid.clone(),
                request_id: r.request_id,
                similarity: r.similarity,
                score: r.similarity - floor,
            });
    }
    out
}

fn assignment_row(
    c: &CorrectionForTagging,
    hits: Vec<AnchorHit>,
    floor: (i64, f64),
    embedder: &EmbedderRow,
    min_score: f64,
) -> (NewAssignment, Vec<AnchorHit>) {
    let correction = CorrectionRef {
        session_uuid: &c.session_uuid,
        request_id: c.request_id,
    };
    let assigned = assign(hits, &correction, min_score);
    let tops = assigned
        .as_ref()
        .map(|a| a.top_anchors.clone())
        .unwrap_or_default();
    (row_from(c, assigned, floor, embedder), tops)
}

fn row_from(
    c: &CorrectionForTagging,
    assigned: Option<super::tagger::Assignment>,
    floor: (i64, f64),
    embedder: &EmbedderRow,
) -> NewAssignment {
    let base = NewAssignment {
        corrected_turn_id: c.corrected_turn_id,
        correction_item_id: c.item_id,
        qualification: c.qualification.clone(),
        qualification_ref: c.qualification_ref.clone(),
        pattern: None,
        score: -floor.1,
        similarity: 0.0,
        anchor_item_id: None,
        runner_up_pattern: None,
        margin: None,
        top_anchors: serde_json::json!([]),
        floor_id: floor.0,
        tagger_version: TAGGER_VERSION.to_owned(),
        embedder_key: embedder.embedder_key.clone(),
    };
    let Some(a) = assigned else {
        return base;
    };
    NewAssignment {
        pattern: a.pattern,
        score: a.score,
        similarity: a.similarity,
        anchor_item_id: a.anchor_item_id,
        runner_up_pattern: a.runner_up_pattern,
        margin: a.margin,
        top_anchors: serde_json::to_value(&a.top_anchors).unwrap_or_else(|_| serde_json::json!([])),
        ..base
    }
}

pub async fn tag_tick(
    persist: &PersistHandle,
    config: &SemanticConfig,
    embedder: &EmbedderRow,
    decisions: Option<&DecisionEnv>,
) -> Result<TagReport, SemanticError> {
    let floors = floor_set(persist, &embedder.embedder_key, config.floor_quantile).await?;
    let Some(floor) = floors.by_pair.get("corr:corr").copied() else {
        return Ok(TagReport {
            no_floor: true,
            ..TagReport::default()
        });
    };
    let corrections = persist
        .knowledge()
        .corrections_needing_tagging(&embedder.embedder_key, TAGGER_VERSION, BATCH)
        .await?;
    let anchors = persist
        .knowledge()
        .nearest_anchors(&corrections, &embedder.embedder_key)
        .await?;
    let mut hits = hits_for(&anchors, floor.floor);
    let (mut rows, tops): (Vec<NewAssignment>, Vec<Vec<AnchorHit>>) = corrections
        .iter()
        .map(|c| {
            let h = hits.remove(&c.item_id).unwrap_or_default();
            assignment_row(
                c,
                h,
                (floor.id, floor.floor),
                embedder,
                config.tagger_min_score,
            )
        })
        .unzip();
    super::tag_decide::refine(decisions, persist, &corrections, &mut rows, &tops).await;
    let assigned = rows.iter().filter(|r| r.pattern.is_some()).count();
    let written = persist
        .knowledge()
        .insert_pattern_assignments(&rows)
        .await?;
    Ok(TagReport {
        corrections: corrections.len(),
        assigned,
        written,
        no_floor: false,
    })
}

fn cluster_floor(row_p: (f64, f64, f64), quantile: f64) -> f64 {
    let (p95, p99, p999) = row_p;
    if quantile >= 0.995 {
        p999
    } else if quantile >= 0.97 {
        p99
    } else {
        p95
    }
}

pub async fn group_weekly(
    persist: &PersistHandle,
    config: &SemanticConfig,
    embedder: &EmbedderRow,
    execute: bool,
) -> Result<Vec<NewCandidate>, SemanticError> {
    let rows = persist
        .knowledge()
        .semantic_floors_in_force(&embedder.embedder_key, config.floor_quantile)
        .await?;
    let Some(corr) = rows.into_iter().find(|r| r.pair == "corr:corr") else {
        return Ok(Vec::new());
    };
    let residue = persist
        .knowledge()
        .pattern_residue(&embedder.embedder_key)
        .await?;
    let members: Vec<Member> = residue
        .iter()
        .map(|r| Member {
            item_id: r.item_id,
            session_id: r.session_id.unwrap_or(0),
            vector: parse_vector(&r.embedding),
        })
        .collect();
    let rules = GroupRules {
        floor: cluster_floor(
            (corr.p95, corr.p99, corr.p999),
            config.cluster_floor_quantile,
        ),
        min_members: config.cluster_min_members,
        min_sessions: config.cluster_min_sessions,
    };
    let now = Utc::now();
    let candidates: Vec<NewCandidate> = mutual_groups(&members, rules)
        .into_iter()
        .map(|g| NewCandidate {
            cluster_version: CLUSTER_VERSION.to_owned(),
            embedder_key: embedder.embedder_key.clone(),
            floor_id: Some(corr.id),
            window_start: now - Duration::days(30),
            window_end: now,
            member_item_ids: g.member_item_ids,
            session_count: i32::try_from(g.session_count).unwrap_or(i32::MAX),
            medoid_item_id: Some(g.medoid_item_id),
        })
        .collect();
    if execute {
        persist
            .knowledge()
            .insert_pattern_candidates(&candidates)
            .await?;
    }
    Ok(candidates)
}
