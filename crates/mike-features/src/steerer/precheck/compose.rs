use std::collections::BTreeMap;

use serde::Serialize;

use super::questions::{SENDS_WHEN_FALSE, SENDS_WHEN_TRUE};
use crate::steerer::guard::Question;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    pub send: bool,
    pub score: f64,
    pub why: String,
}

pub fn score(question: Question, answers: &BTreeMap<String, f64>) -> Option<f64> {
    if question != Question::UnobservedDone {
        return None;
    }
    let mut total = 0.0;
    for key in SENDS_WHEN_TRUE {
        total += answers.get(key).copied()?;
    }
    for key in SENDS_WHEN_FALSE {
        total += 1.0 - answers.get(key).copied()?;
    }
    Some(total / (SENDS_WHEN_TRUE.len() + SENDS_WHEN_FALSE.len()) as f64)
}

pub fn compose(question: Question, answers: &BTreeMap<String, f64>, hold_below: f64) -> Verdict {
    let Some(score) = score(question, answers) else {
        return Verdict {
            send: true,
            score: 1.0,
            why: "no pre-check for this question, or an answer is missing".to_owned(),
        };
    };
    let send = score >= hold_below;
    let why = if send {
        format!(
            "score {score:.2} is at or above {hold_below:.2}: the operator is likely to want the check"
        )
    } else {
        format!(
            "score {score:.2} is below {hold_below:.2}: the operator is likely to accept the turn as it is"
        )
    };
    Verdict { send, score, why }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steerer::precheck::questions::{
        ALREADY_DISCLOSED, ANSWER_NOT_CHANGE, CLAIMS_CHECKED, INVITES_TRIAL,
    };

    fn answers(disclosed: f64, claims: f64, answer: f64, invites: f64) -> BTreeMap<String, f64> {
        [
            (ALREADY_DISCLOSED, disclosed),
            (CLAIMS_CHECKED, claims),
            (ANSWER_NOT_CHANGE, answer),
            (INVITES_TRIAL, invites),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), *v))
        .collect()
    }

    #[test]
    fn an_answer_that_claims_it_was_checked_is_held() {
        let v = compose(Question::UnobservedDone, &answers(0.0, 0.9, 0.9, 0.0), 0.3);
        assert!(!v.send, "{v:?}");
        assert!((v.score - 0.05).abs() < 1e-9);
    }

    #[test]
    fn a_fix_that_names_what_was_not_checked_is_sent() {
        let v = compose(Question::UnobservedDone, &answers(0.9, 0.1, 0.1, 0.6), 0.3);
        assert!(v.send, "{v:?}");
    }

    #[test]
    fn the_threshold_is_inclusive() {
        let v = compose(Question::UnobservedDone, &answers(0.3, 0.7, 0.7, 0.3), 0.3);
        assert!(v.send, "{v:?}");
    }

    #[test]
    fn a_missing_answer_fails_open_to_send() {
        let mut a = answers(0.0, 1.0, 1.0, 0.0);
        a.remove(INVITES_TRIAL);
        assert!(compose(Question::UnobservedDone, &a, 0.3).send);
    }

    #[test]
    fn questions_without_a_precheck_always_send() {
        for q in [
            Question::Relapse,
            Question::AuthorisedHandback,
            Question::UnbackedWait,
        ] {
            assert!(compose(q, &BTreeMap::new(), 0.99).send);
        }
    }
}
