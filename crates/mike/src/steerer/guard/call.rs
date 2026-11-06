use serde::Serialize;

use super::Question;
use super::schema::{AnswerCheck, GuardVerdict, guard_verdict_schema};
use crate::llm::OpenRouterClient;

pub const SYSTEM_PROMPT: &str = include_str!("prompt/system.md");
pub const QUORUM_EXTRA_CALLS: usize = 2;
pub const QUORUM_NEEDED: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub enum CallFailure {
    Refused,
    Failed(String),
}

impl CallFailure {
    pub fn abstention(&self) -> &'static str {
        match self {
            Self::Refused => "governor_refused",
            Self::Failed(_) => "llm_call_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decision {
    pub question: Question,
    pub fires: bool,
    pub abstention: Option<&'static str>,
    pub first_vote_fires: bool,
    pub votes_fire: usize,
    pub votes_total: usize,
    pub confidence: Option<f64>,
    pub reason: Option<String>,
}

pub fn passes(
    verdict: &GuardVerdict,
    question: Question,
    threshold: f64,
) -> Result<(), &'static str> {
    match verdict.check(question) {
        AnswerCheck::Missing => Err("llm_call_failed"),
        AnswerCheck::Declined => Err("verdict_declined"),
        AnswerCheck::NoEvidence => Err("no_evidence"),
        AnswerCheck::Applies => {
            let confidence = verdict.answer(question).map_or(0.0, |a| a.confidence);
            if confidence < threshold {
                Err("confidence_below_threshold")
            } else {
                Ok(())
            }
        }
    }
}

pub fn decide(question: Question, votes: &[GuardVerdict], threshold: f64) -> Decision {
    let first = votes.first();
    let first_result = first.map_or(Err("llm_call_failed"), |v| passes(v, question, threshold));
    let votes_fire = votes
        .iter()
        .filter(|v| passes(v, question, threshold).is_ok())
        .count();
    let quorum = question.has_quorum() && votes.len() > 1;
    let fires = match (&first_result, quorum) {
        (Err(_), _) => false,
        (Ok(()), false) => true,
        (Ok(()), true) => votes_fire == votes.len() && votes_fire >= QUORUM_NEEDED,
    };
    let abstention = match (&first_result, fires) {
        (Err(reason), _) => Some(*reason),
        (Ok(()), false) => Some("quorum_not_reached"),
        (Ok(()), true) => None,
    };
    let answer = first.and_then(|v| v.answer(question));
    Decision {
        question,
        fires,
        abstention,
        first_vote_fires: first_result.is_ok(),
        votes_fire,
        votes_total: votes.len(),
        confidence: answer.map(|a| a.confidence),
        reason: answer.map(|a| a.reason.clone()),
    }
}

pub fn needs_quorum(
    open: &[Question],
    first: &GuardVerdict,
    thresholds: &(dyn Fn(Question) -> f64 + Sync),
) -> bool {
    open.iter()
        .any(|q| q.has_quorum() && passes(first, *q, thresholds(*q)).is_ok())
}

pub async fn ask_once(
    llm: &OpenRouterClient,
    system: &str,
    input: &str,
    open: &[Question],
) -> Result<GuardVerdict, CallFailure> {
    llm.verdict_with_usage::<GuardVerdict>(system, input, guard_verdict_schema(open))
        .await
        .map(|(v, _)| v)
        .map_err(|e| {
            if e.is_refusal() {
                CallFailure::Refused
            } else {
                CallFailure::Failed(e.to_string())
            }
        })
}

pub struct CallRequest<'a> {
    pub system: &'a str,
    pub input: &'a str,
    pub open: &'a [Question],
}

pub async fn ask_with_quorum(
    llm: &OpenRouterClient,
    req: &CallRequest<'_>,
    thresholds: &(dyn Fn(Question) -> f64 + Sync),
) -> Result<Vec<GuardVerdict>, CallFailure> {
    let first = ask_once(llm, req.system, req.input, req.open).await?;
    let mut votes = vec![first];
    if !needs_quorum(req.open, &votes[0], thresholds) {
        return Ok(votes);
    }
    for _ in 0..QUORUM_EXTRA_CALLS {
        match ask_once(llm, req.system, req.input, req.open).await {
            Ok(v) => votes.push(v),
            Err(CallFailure::Refused) => break,
            Err(CallFailure::Failed(e)) => {
                tracing::debug!(error = %e, "turn guard: quorum call failed");
            }
        }
    }
    Ok(votes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vote(relapse: bool, wait: bool) -> GuardVerdict {
        let answer = |applies: bool| json!({"applies": applies, "confidence": 0.9, "evidence": ["req 1"], "quote": "q", "reason": "r"});
        serde_json::from_value(json!({"questions": {"relapse": answer(relapse), "unbacked_wait": answer(wait), "unobserved_done": answer(true)}}))
            .unwrap_or_default()
    }

    #[test]
    fn quorum_counts_votes_per_question() {
        let votes = vec![vote(true, true), vote(true, false), vote(true, false)];
        let relapse = decide(Question::Relapse, &votes, 0.75);
        let wait = decide(Question::UnbackedWait, &votes, 0.75);
        assert!(relapse.fires);
        assert_eq!(relapse.votes_fire, 3);
        assert!(!wait.fires);
        assert_eq!(wait.votes_fire, 1);
        assert_eq!(wait.abstention, Some("quorum_not_reached"));
    }

    #[test]
    fn first_vote_recorded_beside_quorum() {
        let votes = vec![vote(true, true), vote(false, false), vote(false, false)];
        let relapse = decide(Question::Relapse, &votes, 0.75);
        assert!(relapse.first_vote_fires);
        assert!(!relapse.fires);
    }

    #[test]
    fn quorum_only_for_relapse_and_unbacked_wait() {
        let votes = vec![vote(false, false), vote(false, false), vote(false, false)];
        let done = decide(Question::UnobservedDone, &votes, 0.7);
        assert!(done.fires, "unobserved_done fires on its first vote");
        assert!(!Question::UnobservedDone.has_quorum());
        assert!(!Question::AuthorisedHandback.has_quorum());
        let first = vote(false, false);
        assert!(!needs_quorum(&[Question::UnobservedDone], &first, &|_| 0.7));
    }
}
