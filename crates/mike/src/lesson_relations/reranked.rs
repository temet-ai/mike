use super::detection::detect_one_pair;
use super::{
    DetectionReport, LessonRelationsError, RelationJudge, debug_assert_relation_invariants,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

pub(crate) const RERANKED_PAIR_LIMIT: i64 = 100;

#[derive(Debug, Clone, Default)]
pub(crate) struct RerankedDetectionReport {
    pub(crate) detection: DetectionReport,
    pub(crate) handled: Vec<(i64, i64)>,
}

pub(crate) async fn run_detection_on_reranked(
    persist: &PersistHandle,
    judge: &impl RelationJudge,
    project_ids: &[i64],
    dry_run: bool,
) -> Result<RerankedDetectionReport, LessonRelationsError> {
    let mut report = RerankedDetectionReport::default();
    let pairs = persist
        .knowledge()
        .unjudged_lesson_pairs(project_ids, RERANKED_PAIR_LIMIT)
        .await?;
    report.detection.candidate_pairs_considered = pairs.len();
    for pair in &pairs {
        if let Some(detected) = detect_one_pair(persist, judge, pair, dry_run).await? {
            debug_assert_relation_invariants(&detected);
            report.detection.relations.push(detected);
        }
        report.handled.push((pair.lesson_a_id, pair.lesson_b_id));
    }
    if !dry_run {
        persist
            .knowledge()
            .mark_lesson_pairs_judged(&report.handled)
            .await?;
    }
    Ok(report)
}
