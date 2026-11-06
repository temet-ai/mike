use serde::Serialize;

pub const TAGGER_VERSION: &str = "tagger-v1";
pub const TOP_ANCHORS: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnchorHit {
    pub item_id: i64,
    pub pattern: String,
    pub origin_session_uuid: Option<String>,
    pub request_id: Option<i64>,
    pub similarity: f64,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionRef<'a> {
    pub session_uuid: &'a str,
    pub request_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Assignment {
    pub pattern: Option<String>,
    pub anchor_item_id: Option<i64>,
    pub similarity: f64,
    pub score: f64,
    pub runner_up_pattern: Option<String>,
    pub margin: Option<f64>,
    pub top_anchors: Vec<AnchorHit>,
}

pub fn eligible_anchor(hit: &AnchorHit, correction: &CorrectionRef<'_>) -> bool {
    let same_session = hit.origin_session_uuid.as_deref() == Some(correction.session_uuid);
    let self_anchor = correction.request_id.is_some() && hit.request_id == correction.request_id;
    !same_session && !self_anchor
}

pub fn assign(
    mut hits: Vec<AnchorHit>,
    correction: &CorrectionRef<'_>,
    min_score: f64,
) -> Option<Assignment> {
    hits.retain(|h| eligible_anchor(h, correction));
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.item_id.cmp(&b.item_id)));
    let hits: Vec<AnchorHit> = hits.into_iter().take(TOP_ANCHORS).collect();
    let top = hits.first()?.clone();
    let runner_up = hits.iter().find(|h| h.pattern != top.pattern);
    Some(Assignment {
        pattern: (top.score >= min_score).then(|| top.pattern.clone()),
        anchor_item_id: Some(top.item_id),
        similarity: top.similarity,
        score: top.score,
        runner_up_pattern: runner_up.map(|h| h.pattern.clone()),
        margin: runner_up.map(|h| top.score - h.score),
        top_anchors: hits,
    })
}
