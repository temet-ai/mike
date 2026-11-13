use crate::store::persistence::lessons_repo::LessonsPersist as _;
use serde_json::json;

use super::evidence::{EvidenceBundle, EvidenceBundleParams};
use super::rules::shared_subject_tokens;
use super::schema::{AdjudicatorVerdict, ProposerVerdict};
use super::{
    DETECTOR_VERSION, DetectedRelation, KIND_SUPERSEDES, LessonRelationsError, METHOD_ADJUDICATED,
    RelationJudge, STATUS_CONTESTED, STATUS_PROPOSED,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lesson_relations::{CandidateLessonPair, LessonRelationDraft};

const VALID_KINDS: [&str; 4] = [
    KIND_SUPERSEDES,
    super::KIND_REFINES,
    super::KIND_RECURS,
    super::KIND_CONTRADICTS,
];

const RULE_RESOLUTION_POLARITY: &str = "rule:resolution-polarity-v1";

#[derive(serde::Serialize)]
struct ResolutionSupersessionEvidence<'a> {
    #[serde(flatten)]
    bundle: &'a EvidenceBundle,
    shared_subject_tokens: &'a [String],
}

fn build_bundle(pair: &CandidateLessonPair) -> EvidenceBundle {
    EvidenceBundle::build(EvidenceBundleParams {
        lesson_a_id: pair.lesson_a_id,
        lesson_a_text: pair.lesson_a_text.clone(),
        lesson_a_created_at: pair.lesson_a_created_at,
        lesson_b_id: pair.lesson_b_id,
        lesson_b_text: pair.lesson_b_text.clone(),
        lesson_b_created_at: pair.lesson_b_created_at,
        cosine_distance: pair.distance,
    })
}

pub async fn run_adjudicated_pair(
    judge: &impl RelationJudge,
    pair: &CandidateLessonPair,
) -> Option<(EvidenceBundle, ProposerVerdict, AdjudicatorVerdict)> {
    let bundle = build_bundle(pair);
    let prompt = bundle.to_prompt();

    let proposal = call_proposer(judge, &prompt, pair).await?;
    if !proposal_is_actionable(&proposal, pair, &bundle) {
        return None;
    }

    let adjudicator_prompt = build_adjudicator_prompt(&prompt, &bundle, &proposal);
    let verdict = call_adjudicator(judge, &adjudicator_prompt, pair).await?;

    Some((bundle, proposal, verdict))
}

async fn call_proposer(
    judge: &impl RelationJudge,
    prompt: &str,
    pair: &CandidateLessonPair,
) -> Option<ProposerVerdict> {
    match judge.propose(prompt).await {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(
                error = %e,
                lesson_a_id = pair.lesson_a_id,
                lesson_b_id = pair.lesson_b_id,
                "lesson_relations: proposer call failed — skipping pair"
            );
            None
        }
    }
}

fn proposal_is_actionable(
    proposal: &ProposerVerdict,
    pair: &CandidateLessonPair,
    bundle: &EvidenceBundle,
) -> bool {
    if !proposal.is_related {
        return false;
    }
    if !proposal_kind_is_valid(proposal) {
        return false;
    }
    if !proposal_ids_within_pair(proposal, pair) {
        return false;
    }
    proposal_quotes_are_verifiable(proposal, pair, bundle)
}

fn proposal_kind_is_valid(proposal: &ProposerVerdict) -> bool {
    let valid = VALID_KINDS.contains(&proposal.kind.as_str());
    if !valid {
        tracing::warn!(
            kind = %proposal.kind,
            "lesson_relations: proposer returned an unknown kind — skipping pair"
        );
    }
    valid
}

fn proposal_quotes_are_verifiable(
    proposal: &ProposerVerdict,
    pair: &CandidateLessonPair,
    bundle: &EvidenceBundle,
) -> bool {
    let verifiable = !proposal.quoted_evidence.is_empty()
        && proposal
            .quoted_evidence
            .iter()
            .all(|q| bundle.quote_is_verifiable(q));
    if !verifiable {
        tracing::warn!(
            lesson_a_id = pair.lesson_a_id,
            lesson_b_id = pair.lesson_b_id,
            "lesson_relations: proposer cited a quote that does not exist verbatim in either lesson — skipping unverifiable proposal"
        );
    }
    verifiable
}

fn proposal_ids_within_pair(proposal: &ProposerVerdict, pair: &CandidateLessonPair) -> bool {
    let expected = [pair.lesson_a_id, pair.lesson_b_id];
    let valid = expected.contains(&proposal.from_lesson_id)
        && expected.contains(&proposal.to_lesson_id)
        && proposal.from_lesson_id != proposal.to_lesson_id;
    if !valid {
        tracing::warn!(
            from = proposal.from_lesson_id,
            to = proposal.to_lesson_id,
            a = pair.lesson_a_id,
            b = pair.lesson_b_id,
            "lesson_relations: proposer returned ids outside the candidate pair — skipping"
        );
    }
    valid
}

fn build_adjudicator_prompt(
    prompt: &str,
    bundle: &EvidenceBundle,
    proposal: &ProposerVerdict,
) -> String {
    let side_of = |id: i64| {
        if id == bundle.lesson_a_id {
            "LESSON A"
        } else {
            "LESSON B"
        }
    };
    let from_side = side_of(proposal.from_lesson_id);
    let to_side = side_of(proposal.to_lesson_id);
    format!(
        "{prompt}\nPROPOSER'S CLAIM:\n  \
         {from_side} (id={from}) {kind} {to_side} (id={to})\n  \
         is_related: {is_related}\n  kind: {kind}\n  \
         from_lesson_id: {from}  (this is {from_side} above)\n  \
         to_lesson_id: {to}  (this is {to_side} above)\n  \
         rationale: {rationale:?}\n  quoted_evidence: {quotes:?}\n",
        is_related = proposal.is_related,
        kind = proposal.kind,
        from = proposal.from_lesson_id,
        to = proposal.to_lesson_id,
        rationale = proposal.rationale,
        quotes = proposal.quoted_evidence,
    )
}

async fn call_adjudicator(
    judge: &impl RelationJudge,
    adjudicator_prompt: &str,
    pair: &CandidateLessonPair,
) -> Option<AdjudicatorVerdict> {
    match judge.adjudicate(adjudicator_prompt).await {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(
                error = %e,
                lesson_a_id = pair.lesson_a_id,
                lesson_b_id = pair.lesson_b_id,
                "lesson_relations: adjudicator call failed — skipping pair"
            );
            None
        }
    }
}

pub async fn run_rule_based_pair(
    persist: &PersistHandle,
    judge: &impl RelationJudge,
    pair: &CandidateLessonPair,
    from_lesson_id: i64,
    to_lesson_id: i64,
    dry_run: bool,
) -> Result<Option<DetectedRelation>, LessonRelationsError> {
    let bundle = build_bundle(pair);
    let shared_tokens = shared_subject_tokens(&pair.lesson_a_text, &pair.lesson_b_text);
    let rationale = format!(
        "deterministic rule ({RULE_RESOLUTION_POLARITY}): lesson {from_lesson_id} carries \
         resolution-polarity markers lesson {to_lesson_id} lacks, is strictly newer, and shares \
         {} subject terms with it",
        shared_tokens.len()
    );
    let adjudicator_prompt = format!(
        "{base}\nDETERMINISTIC PROPOSAL ({RULE_RESOLUTION_POLARITY}):\n  kind: supersedes\n  \
         from_lesson_id: {from_lesson_id}\n  to_lesson_id: {to_lesson_id}\n  rationale: {rationale:?}\n",
        base = bundle.to_prompt(),
    );

    let verdict = match judge.adjudicate(&adjudicator_prompt).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                error = %e,
                lesson_a_id = pair.lesson_a_id,
                lesson_b_id = pair.lesson_b_id,
                "lesson_relations: adjudicator call failed for a rule-based proposal — skipping pair"
            );
            return Ok(None);
        }
    };

    let adjudicator_agrees =
        verdict.from_lesson_id == from_lesson_id && verdict.to_lesson_id == to_lesson_id;
    let status = if adjudicator_agrees {
        STATUS_PROPOSED
    } else {
        STATUS_CONTESTED
    };
    let evidence_json = serde_json::to_value(ResolutionSupersessionEvidence {
        bundle: &bundle,
        shared_subject_tokens: &shared_tokens,
    })
    .unwrap_or_else(|_| json!({}));

    let detected = DetectedRelation {
        from_lesson_id,
        to_lesson_id,
        kind: KIND_SUPERSEDES.to_string(),
        status,
    };

    if !dry_run {
        persist
            .lessons()
            .insert_lesson_relation(&LessonRelationDraft {
                from_lesson_id,
                to_lesson_id,
                kind: KIND_SUPERSEDES,
                method: METHOD_ADJUDICATED,
                status,
                candidate_distance: Some(pair.distance),
                evidence_json,
                proposer_model: Some(RULE_RESOLUTION_POLARITY),
                proposer_rationale: Some(&rationale),
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
