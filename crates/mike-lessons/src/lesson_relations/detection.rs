use crate::store::persistence::lessons_repo::LessonsPersist as _;
use serde_json::json;

use super::adjudication::{run_adjudicated_pair, run_rule_based_pair};
use super::decide;
use super::evidence::{EvidenceBundle, EvidenceBundleParams};
use super::rules::{find_artifact_reference, find_resolution_supersession};
use super::{
    DETECTOR_VERSION, DetectedRelation, KIND_SUPERSEDES, LessonRelationsError, METHOD_ADJUDICATED,
    METHOD_ARTIFACT_REFERENCE, RelationJudge, STATUS_CONTESTED, STATUS_PROPOSED,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lesson_relations::{CandidateLessonPair, LessonRelationDraft};

struct ArtifactCitation<'a> {
    from_lesson_id: i64,
    to_lesson_id: i64,
    quote: &'a str,
    char_start: usize,
    char_end: usize,
}

#[derive(serde::Serialize)]
struct ArtifactReferenceEvidence<'a> {
    #[serde(flatten)]
    bundle: &'a EvidenceBundle,
    citation_quote: &'a str,
    citation_char_start: usize,
    citation_char_end: usize,
}

#[derive(serde::Serialize)]
struct AdjudicatedEvidence<'a> {
    #[serde(flatten)]
    bundle: &'a EvidenceBundle,
    proposer_quoted_evidence: &'a [String],
}

pub async fn detect_one_pair(
    persist: &PersistHandle,
    judge: &impl RelationJudge,
    pair: &CandidateLessonPair,
    dry_run: bool,
) -> Result<Option<DetectedRelation>, LessonRelationsError> {
    if let Some(jev) = judge.jev()
        && let Some((decision, record_id)) = decide::decide(jev, pair).await
    {
        return apply_decision(persist, jev, pair, (decision, record_id), dry_run).await;
    }
    if let Some((quote, char_start, char_end)) =
        find_artifact_reference(&pair.lesson_b_text, pair.lesson_a_id)
    {
        let citation = ArtifactCitation {
            from_lesson_id: pair.lesson_b_id,
            to_lesson_id: pair.lesson_a_id,
            quote: &quote,
            char_start,
            char_end,
        };
        return insert_artifact_reference(persist, pair, citation, dry_run)
            .await
            .map(Some);
    }
    if let Some((quote, char_start, char_end)) =
        find_artifact_reference(&pair.lesson_a_text, pair.lesson_b_id)
    {
        let citation = ArtifactCitation {
            from_lesson_id: pair.lesson_a_id,
            to_lesson_id: pair.lesson_b_id,
            quote: &quote,
            char_start,
            char_end,
        };
        return insert_artifact_reference(persist, pair, citation, dry_run)
            .await
            .map(Some);
    }

    if let Some((from_lesson_id, to_lesson_id)) = find_resolution_supersession(pair) {
        return run_rule_based_pair(persist, judge, pair, from_lesson_id, to_lesson_id, dry_run)
            .await;
    }

    let Some((bundle, proposal, verdict)) = run_adjudicated_pair(judge, pair).await else {
        return Ok(None);
    };

    let adjudicator_agrees = verdict.from_lesson_id == proposal.from_lesson_id
        && verdict.to_lesson_id == proposal.to_lesson_id;
    let status = if adjudicator_agrees {
        STATUS_PROPOSED
    } else {
        STATUS_CONTESTED
    };
    let evidence_json = serde_json::to_value(AdjudicatedEvidence {
        bundle: &bundle,
        proposer_quoted_evidence: &proposal.quoted_evidence,
    })
    .unwrap_or_else(|_| json!({}));

    let detected = DetectedRelation {
        from_lesson_id: proposal.from_lesson_id,
        to_lesson_id: proposal.to_lesson_id,
        kind: proposal.kind.clone(),
        status,
    };

    if !dry_run {
        persist
            .lessons()
            .insert_lesson_relation(&LessonRelationDraft {
                from_lesson_id: proposal.from_lesson_id,
                to_lesson_id: proposal.to_lesson_id,
                kind: &proposal.kind,
                method: METHOD_ADJUDICATED,
                status,
                candidate_distance: Some(pair.distance),
                evidence_json,
                proposer_model: Some(judge.proposer_model()),
                proposer_rationale: Some(&proposal.rationale),
                adjudicator_model: Some(judge.adjudicator_model()),
                adjudicator_agrees: Some(adjudicator_agrees),
                adjudicator_rationale: Some(&verdict.rationale),
                detector_version: DETECTOR_VERSION,
                decided_by: None,
            })
            .await?;
    }

    Ok(Some(detected))
}

async fn apply_decision(
    persist: &PersistHandle,
    jev: &decide::PairJev,
    pair: &CandidateLessonPair,
    decided: (decide::PairDecision, Option<i64>),
    dry_run: bool,
) -> Result<Option<DetectedRelation>, LessonRelationsError> {
    let (decision, record_id) = decided;
    match decision {
        decide::PairDecision::Unrelated(_) => Ok(None),
        decide::PairDecision::Cites(cite) => {
            let citation = ArtifactCitation {
                from_lesson_id: cite.from_lesson_id,
                to_lesson_id: cite.to_lesson_id,
                quote: &cite.quote,
                char_start: cite.char_start,
                char_end: cite.char_end,
            };
            insert_artifact_reference(persist, pair, citation, dry_run)
                .await
                .map(Some)
        }
        decide::PairDecision::Relation {
            from_lesson_id,
            to_lesson_id,
            kind,
            probability,
        } => {
            let bundle = EvidenceBundle::build(EvidenceBundleParams {
                lesson_a_id: pair.lesson_a_id,
                lesson_a_text: pair.lesson_a_text.clone(),
                lesson_a_created_at: pair.lesson_a_created_at,
                lesson_b_id: pair.lesson_b_id,
                lesson_b_text: pair.lesson_b_text.clone(),
                lesson_b_created_at: pair.lesson_b_created_at,
                cosine_distance: pair.distance,
            });
            let mut evidence = serde_json::to_value(&bundle).unwrap_or_else(|_| json!({}));
            if let Some(map) = evidence.as_object_mut() {
                map.insert("decision_record_id".to_owned(), json!(record_id));
                map.insert("jev_probability".to_owned(), json!(probability));
            }
            let rationale = format!("Jev chose {kind} with probability {probability:.2}");
            if !dry_run {
                persist
                    .lessons()
                    .insert_lesson_relation(&LessonRelationDraft {
                        from_lesson_id,
                        to_lesson_id,
                        kind,
                        method: METHOD_ADJUDICATED,
                        status: STATUS_PROPOSED,
                        candidate_distance: Some(pair.distance),
                        evidence_json: evidence,
                        proposer_model: Some(jev.model()),
                        proposer_rationale: Some(&rationale),
                        adjudicator_model: None,
                        adjudicator_agrees: None,
                        adjudicator_rationale: None,
                        detector_version: DETECTOR_VERSION,
                        decided_by: None,
                    })
                    .await?;
            }
            Ok(Some(DetectedRelation {
                from_lesson_id,
                to_lesson_id,
                kind: kind.to_owned(),
                status: STATUS_PROPOSED,
            }))
        }
    }
}

async fn insert_artifact_reference(
    persist: &PersistHandle,
    pair: &CandidateLessonPair,
    citation: ArtifactCitation<'_>,
    dry_run: bool,
) -> Result<DetectedRelation, LessonRelationsError> {
    let bundle = EvidenceBundle::build(EvidenceBundleParams {
        lesson_a_id: pair.lesson_a_id,
        lesson_a_text: pair.lesson_a_text.clone(),
        lesson_a_created_at: pair.lesson_a_created_at,
        lesson_b_id: pair.lesson_b_id,
        lesson_b_text: pair.lesson_b_text.clone(),
        lesson_b_created_at: pair.lesson_b_created_at,
        cosine_distance: pair.distance,
    });
    let evidence_json = serde_json::to_value(ArtifactReferenceEvidence {
        bundle: &bundle,
        citation_quote: citation.quote,
        citation_char_start: citation.char_start,
        citation_char_end: citation.char_end,
    })
    .unwrap_or_else(|_| json!({}));
    let rationale = format!(
        "lesson {} explicitly cites lesson {}: {:?}",
        citation.from_lesson_id, citation.to_lesson_id, citation.quote
    );

    let detected = DetectedRelation {
        from_lesson_id: citation.from_lesson_id,
        to_lesson_id: citation.to_lesson_id,
        kind: KIND_SUPERSEDES.to_string(),
        status: STATUS_PROPOSED,
    };

    if !dry_run {
        persist
            .lessons()
            .insert_lesson_relation(&LessonRelationDraft {
                from_lesson_id: citation.from_lesson_id,
                to_lesson_id: citation.to_lesson_id,
                kind: KIND_SUPERSEDES,
                method: METHOD_ARTIFACT_REFERENCE,
                status: STATUS_PROPOSED,
                candidate_distance: Some(pair.distance),
                evidence_json,
                proposer_model: None,
                proposer_rationale: Some(&rationale),
                adjudicator_model: None,
                adjudicator_agrees: None,
                adjudicator_rationale: None,
                detector_version: DETECTOR_VERSION,
                decided_by: None,
            })
            .await?;
    }

    Ok(detected)
}
