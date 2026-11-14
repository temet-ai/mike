use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashMap;

use super::label_rules::is_prod;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::steerer::prod_point;
use crate::store::persistence::steering::guard::GuardLabelCandidate;

const MEMO_LIMIT: usize = 10_000;

static ANSWERS: LazyLock<DashMap<i64, bool>> = LazyLock::new(DashMap::new);

#[derive(Default)]
pub struct ProdAnswers {
    decider: Option<Decider>,
    stopped: AtomicBool,
}

impl ProdAnswers {
    pub fn new(decider: Option<Decider>) -> Self {
        Self {
            decider,
            stopped: AtomicBool::new(false),
        }
    }

    pub async fn of(&self, c: &GuardLabelCandidate) -> Option<bool> {
        let message = c.next_genuine_text.as_deref()?;
        let Some(decider) = self.decider.as_ref() else {
            return Some(is_prod(message));
        };
        if let Some(known) = ANSWERS.get(&c.operator_turn_id) {
            return Some(*known);
        }
        if self.stopped.load(Ordering::Relaxed) {
            return Some(is_prod(message));
        }
        let detected = prod_point::detect_checked(
            Some(decider),
            message,
            DecisionSubject {
                session_id: Some(c.session_id),
                operator_turn_id: Some(c.operator_turn_id),
                request_id: c.next_genuine_request_id,
                ..DecisionSubject::default()
            },
        )
        .await;
        if detected.failed {
            self.stopped.store(true, Ordering::Relaxed);
            return Some(detected.is_prod);
        }
        if ANSWERS.len() > MEMO_LIMIT {
            ANSWERS.clear();
        }
        ANSWERS.insert(c.operator_turn_id, detected.is_prod);
        Some(detected.is_prod)
    }
}
