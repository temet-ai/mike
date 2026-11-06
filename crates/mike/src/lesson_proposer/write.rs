use serde_json::Value;

use crate::lesson_shape::assess;
use crate::steerer::guard::label_rules::is_prod;
use crate::store::persistence::PersistError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::placement::PlacementWrite;
use crate::store::persistence::lessons::proposals::{
    CorrectionCandidate, EvidenceWrite, NewCandidateLesson, ProposalError,
};
use crate::store::persistence::lessons::write_time::NewSkippedDuplicate;

use super::ProposerSettings;
use super::dedupe::Embedded;
use super::prompt::ProposedLesson;
use super::relate::{WriteAction, WriteTimeVerdict};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    EvidenceOnly,
    AwaitingEmbedding,
    Created { lesson_id: i64, promoted: bool },
    Reinforced { lesson_id: i64, promoted: bool },
}

pub struct Decision<'a> {
    pub candidate: &'a CorrectionCandidate,
    pub proposed: &'a ProposedLesson,
    pub embedding: Option<&'a Embedded>,
    pub verdict: Option<&'a WriteTimeVerdict>,
    pub model: &'a str,
    pub cost_usd: Option<f64>,
    pub generation_id: Option<&'a str>,
    pub similar: &'a Value,
}

pub fn accepted_rule<'a>(
    candidate: &CorrectionCandidate,
    proposed: &'a ProposedLesson,
) -> Option<&'a str> {
    let prod = candidate.correcting_message.as_deref().is_some_and(is_prod);
    if prod {
        return None;
    }
    proposed.generalising_rule()
}

impl Decision<'_> {
    fn evidence(&self, lesson_id: Option<i64>) -> EvidenceWrite<'_> {
        EvidenceWrite {
            verdict_id: self.candidate.verdict_id,
            operator_turn_id: self.candidate.operator_turn_id,
            lesson_id,
            similar: self.similar,
            model: self.model,
            cost_usd: self.cost_usd,
            generation_id: self.generation_id,
            reason: &self.proposed.reason,
        }
    }

    fn accepted_rule(&self) -> Option<&str> {
        accepted_rule(self.candidate, self.proposed)
    }
}

const NOT_EXAMINED_REASON: &str = "confidence below threshold; not sent to the proposer";
const NOT_EXAMINED_MODEL: &str = "none";

pub async fn record_low_confidence_evidence(
    persist: &PersistHandle,
    candidate: &CorrectionCandidate,
) -> Result<(), PersistError> {
    let empty = Value::Array(Vec::new());
    persist
        .record_evidence(EvidenceWrite {
            verdict_id: candidate.verdict_id,
            operator_turn_id: candidate.operator_turn_id,
            lesson_id: None,
            similar: &empty,
            model: NOT_EXAMINED_MODEL,
            cost_usd: None,
            generation_id: None,
            reason: NOT_EXAMINED_REASON,
        })
        .await
        .map(|_evidence_id| ())
}

pub async fn decide_and_write(
    persist: &PersistHandle,
    settings: &ProposerSettings,
    decision: &Decision<'_>,
) -> Result<WriteOutcome, PersistError> {
    if decision.candidate.confidence < settings.min_confidence {
        persist.record_evidence(decision.evidence(None)).await?;
        return Ok(WriteOutcome::EvidenceOnly);
    }

    let Some(rule) = decision.accepted_rule() else {
        persist.record_evidence(decision.evidence(None)).await?;
        return Ok(WriteOutcome::EvidenceOnly);
    };

    if let Some(WriteAction::Skip { target }) = decision.verdict.map(|v| &v.action) {
        return skip(persist, settings, decision, rule, *target).await;
    }

    if let Some(WriteAction::Discard { covered_by }) = decision.verdict.map(|v| &v.action) {
        return discard(persist, decision, covered_by).await;
    }

    let Some(embedded) = decision.embedding else {
        return Ok(WriteOutcome::AwaitingEmbedding);
    };

    create(persist, settings, decision, rule, embedded).await
}

async fn discard(
    persist: &PersistHandle,
    decision: &Decision<'_>,
    covered_by: &str,
) -> Result<WriteOutcome, PersistError> {
    let reason = format!(
        "not stored, already covered by {covered_by}: {}",
        decision.proposed.reason
    );
    let mut evidence = decision.evidence(None);
    evidence.reason = &reason;
    persist.record_evidence(evidence).await?;
    Ok(WriteOutcome::EvidenceOnly)
}

async fn skip(
    persist: &PersistHandle,
    settings: &ProposerSettings,
    decision: &Decision<'_>,
    rule: &str,
    target: i64,
) -> Result<WriteOutcome, PersistError> {
    let Some(relation) = decision.verdict.and_then(|v| v.relations.first()) else {
        return Err(PersistError::DataInvariant(format!(
            "skip decision for lesson {target} carries no relation"
        )));
    };
    let candidate = decision.candidate;
    persist
        .record_skipped_duplicate(
            NewSkippedDuplicate {
                project_id: candidate.project_id,
                kind: &decision.proposed.kind,
                text: rule,
                source_turn_id: candidate.operator_turn_id,
                session_db_id: candidate.session_id,
                embedding: decision
                    .embedding
                    .map(|e| (e.vector.as_slice(), e.model.as_str())),
                target_id: target,
                model: decision.model,
                relation,
            },
            decision.evidence(None),
        )
        .await?;
    let promoted = maybe_promote(persist, settings, target).await?;
    Ok(WriteOutcome::Reinforced {
        lesson_id: target,
        promoted,
    })
}

async fn create(
    persist: &PersistHandle,
    settings: &ProposerSettings,
    decision: &Decision<'_>,
    rule: &str,
    embedded: &Embedded,
) -> Result<WriteOutcome, PersistError> {
    let candidate = decision.candidate;
    let text = decision
        .verdict
        .and_then(WriteTimeVerdict::replacement)
        .unwrap_or(rule);
    let assessment = assess(text, None, settings.max_chars);
    let relations = decision.verdict.map_or(&[][..], |v| v.relations.as_slice());
    let placement = decision.verdict.map(|v| PlacementWrite {
        home: v.placement.home.code(),
        covered_by: v.placement.covered_by.as_deref(),
        reason: &v.placement.reason,
    });
    let lesson_id = persist
        .create_candidate_lesson(
            NewCandidateLesson {
                project_id: candidate.project_id,
                kind: &decision.proposed.kind,
                text,
                source_turn_id: candidate.operator_turn_id,
                session_db_id: candidate.session_id,
                request_id: None,
                embedding: &embedded.vector,
                embedding_model: &embedded.model,
                shape: Some(&assessment),
                relations,
                placement,
            },
            decision.evidence(None),
        )
        .await?;
    let promoted = maybe_promote(persist, settings, lesson_id).await?;
    Ok(WriteOutcome::Created {
        lesson_id,
        promoted,
    })
}

async fn maybe_promote(
    persist: &PersistHandle,
    settings: &ProposerSettings,
    lesson_id: i64,
) -> Result<bool, PersistError> {
    if settings.auto_promote_min_evidence <= 0 {
        return Ok(false);
    }
    if persist.lesson_has_open_shape_finding(lesson_id).await? {
        return Ok(false);
    }
    let count = persist.count_promotion_evidence(lesson_id).await?;
    if count < settings.auto_promote_min_evidence {
        return Ok(false);
    }
    match persist
        .approve_lesson_candidate(lesson_id, None, "judge")
        .await
    {
        Ok(()) => Ok(true),
        Err(ProposalError::NotPending | ProposalError::NotFound) => Ok(false),
        Err(ProposalError::Persist(e)) => Err(e),
    }
}
