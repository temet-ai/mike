use crate::store::persistence::lessons_repo::LessonsPersist as _;
pub(crate) mod evidence;

mod adjudication;
mod detection;
mod reranked;
mod rules;
mod schema;

#[cfg(test)]
mod detection_tests;
#[cfg(test)]
mod reranked_tests;

pub(crate) use reranked::run_detection_on_reranked;

use crate::llm::{LlmError, OpenRouterClient};
use crate::store::persistence::PersistError;
use schema::{
    ADJUDICATOR_SYSTEM_PROMPT, AdjudicatorVerdict, PROPOSER_SYSTEM_PROMPT, ProposerVerdict,
    adjudicator_schema, proposer_schema,
};

pub(crate) const DETECTOR_VERSION: &str = "lesson-relations-v1";

pub(crate) const KIND_SUPERSEDES: &str = "supersedes";
pub(crate) const KIND_REFINES: &str = "refines";
pub(crate) const KIND_RECURS: &str = "recurs";
pub(crate) const KIND_CONTRADICTS: &str = "contradicts";

pub(crate) const METHOD_DECLARED: &str = "declared";
pub(crate) const METHOD_ARTIFACT_REFERENCE: &str = "artifact_reference";
pub(crate) const METHOD_ADJUDICATED: &str = "adjudicated";

pub(crate) const STATUS_CONFIRMED: &str = "confirmed";
pub(crate) const STATUS_PROPOSED: &str = "proposed";
pub(crate) const STATUS_CONTESTED: &str = "contested";

#[derive(Debug, thiserror::Error)]
pub(crate) enum LessonRelationsError {
    #[error("proposer and adjudicator must use different models; both resolved to {0:?}")]
    SameModel(String),

    #[error("persistence error: {0}")]
    Persist(#[from] PersistError),
}

pub(crate) fn assert_models_differ(
    proposer_model: &str,
    adjudicator_model: &str,
) -> Result<(), LessonRelationsError> {
    if proposer_model == adjudicator_model {
        Err(LessonRelationsError::SameModel(proposer_model.to_string()))
    } else {
        Ok(())
    }
}

pub(crate) struct ModelParticipant<'a> {
    pub(crate) client: &'a OpenRouterClient,
    pub(crate) model: &'a str,
}

pub(crate) trait RelationJudge {
    fn propose(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<ProposerVerdict, LlmError>>;
    fn adjudicate(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<AdjudicatorVerdict, LlmError>>;

    fn proposer_model(&self) -> &str;
    fn adjudicator_model(&self) -> &str;
}

pub(crate) struct ModelPairJudge<'a> {
    pub(crate) proposer: &'a ModelParticipant<'a>,
    pub(crate) adjudicator: &'a ModelParticipant<'a>,
}

impl RelationJudge for ModelPairJudge<'_> {
    async fn propose(&self, prompt: &str) -> Result<ProposerVerdict, LlmError> {
        self.proposer
            .client
            .verdict::<ProposerVerdict>(PROPOSER_SYSTEM_PROMPT, prompt, proposer_schema())
            .await
    }

    async fn adjudicate(&self, prompt: &str) -> Result<AdjudicatorVerdict, LlmError> {
        self.adjudicator
            .client
            .verdict::<AdjudicatorVerdict>(ADJUDICATOR_SYSTEM_PROMPT, prompt, adjudicator_schema())
            .await
    }

    fn proposer_model(&self) -> &str {
        self.proposer.model
    }

    fn adjudicator_model(&self) -> &str {
        self.adjudicator.model
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DetectedRelation {
    pub(crate) from_lesson_id: i64,
    pub(crate) to_lesson_id: i64,
    pub(crate) kind: String,
    pub(crate) status: &'static str,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DetectionReport {
    pub(crate) candidate_pairs_considered: usize,
    pub(crate) relations: Vec<DetectedRelation>,
}

pub(crate) async fn run_detection(
    persist: &crate::store::persistence::PersistHandle,
    judge: &impl RelationJudge,
    distance_threshold: f64,
    project_ids: &[i64],
    dry_run: bool,
    already_handled: &[(i64, i64)],
) -> Result<DetectionReport, LessonRelationsError> {
    let mut report = DetectionReport::default();

    for &project_id in project_ids {
        let pairs = persist
            .lessons()
            .find_candidate_relation_pairs(project_id, distance_threshold)
            .await?;
        let pairs: Vec<_> = pairs
            .into_iter()
            .filter(|pair| !already_handled.contains(&(pair.lesson_a_id, pair.lesson_b_id)))
            .collect();
        report.candidate_pairs_considered += pairs.len();

        for pair in &pairs {
            if let Some(detected) =
                detection::detect_one_pair(persist, judge, pair, dry_run).await?
            {
                debug_assert_relation_invariants(&detected);
                report.relations.push(detected);
            }
        }
    }

    Ok(report)
}

fn debug_assert_relation_invariants(detected: &DetectedRelation) {
    debug_assert_ne!(
        detected.from_lesson_id, detected.to_lesson_id,
        "a lesson relation must not point a lesson at itself"
    );
    debug_assert!(
        !detected.kind.is_empty(),
        "a lesson relation must carry a non-empty kind"
    );
    debug_assert!(
        matches!(detected.status, STATUS_PROPOSED | STATUS_CONTESTED),
        "a freshly detected relation must be proposed or contested, got {:?}",
        detected.status
    );
}
