use serde_json::{Value, json};

use super::questions::{
    ABANDONED, ACCEPTED, ASKS_OPERATOR, CHECKED, CORRECTED, DELIVERED, LEFT_OPEN, OWN_ERROR,
    REDIRECTED, SHIPPED, Shape,
};
use crate::analysis::judge::two_axis::{Delivery, Reaction};
use crate::analysis::judge::{EvidenceQuality, JudgeLabel};
use crate::llm::decisions::point::Asked;

pub const YES: f64 = 0.5;
pub const CHECKED_AT: f64 = 0.6;
pub const UNCERTAIN_BELOW: f64 = 0.7;

#[derive(Debug, Clone, PartialEq)]
pub struct Composed {
    pub label: JudgeLabel,
    pub reaction: Reaction,
    pub delivery: Delivery,
    pub confidence: f64,
    pub evidence: EvidenceQuality,
}

impl Composed {
    pub fn uncertain(&self) -> bool {
        self.confidence < UNCERTAIN_BELOW
    }

    pub fn summary(&self) -> String {
        format!(
            "decided in code from yes/no answers: label {}, reaction {}, delivery {}, confidence {:.2}",
            self.label.outcome_code(),
            self.reaction.code(),
            self.delivery.code(),
            self.confidence
        )
    }

    pub fn json(&self, threshold: f32) -> Value {
        json!({
            "score": self.confidence,
            "label": self.label.outcome_code(),
            "reaction": self.reaction.code(),
            "delivery": self.delivery.code(),
            "evidence_quality": self.evidence.as_db_code(),
            "confidence": self.confidence,
            "confidence_threshold": threshold,
            "uncertain": self.uncertain(),
        })
    }
}

fn p(asked: &Asked, key: &str) -> f64 {
    asked.noul(key).unwrap_or(0.0)
}

fn reaction(asked: &Asked, has_next: bool) -> (Reaction, f64) {
    if !has_next {
        return (Reaction::NoResponse, 1.0);
    }
    let (corrected, redirected, accepted) = (
        p(asked, CORRECTED),
        p(asked, REDIRECTED),
        p(asked, ACCEPTED),
    );
    if corrected >= YES {
        (Reaction::Corrected, corrected)
    } else if redirected >= YES {
        (Reaction::Redirected, redirected)
    } else if accepted >= YES {
        (Reaction::Accepted, accepted)
    } else {
        (
            Reaction::Unclear,
            1.0 - corrected.max(redirected).max(accepted),
        )
    }
}

fn delivery(asked: &Asked) -> Delivery {
    let delivered = p(asked, DELIVERED) >= YES;
    if p(asked, ABANDONED) >= YES {
        Delivery::None
    } else if p(asked, ASKS_OPERATOR) >= YES && !delivered {
        Delivery::Question
    } else if p(asked, LEFT_OPEN) >= YES {
        Delivery::Partial
    } else if delivered && p(asked, SHIPPED) >= YES {
        Delivery::Shipped
    } else if delivered {
        Delivery::Delivered
    } else {
        Delivery::None
    }
}

fn legacy(asked: &Asked) -> (JudgeLabel, f64) {
    let (error, abandoned, delivered, shipped) = (
        p(asked, OWN_ERROR),
        p(asked, ABANDONED),
        p(asked, DELIVERED),
        p(asked, SHIPPED),
    );
    if error >= YES {
        (JudgeLabel::Corrected, error)
    } else if abandoned >= YES {
        (JudgeLabel::Abandoned, abandoned)
    } else if delivered >= YES && shipped >= YES {
        (JudgeLabel::Shipped, delivered.min(shipped))
    } else if delivered >= YES {
        (JudgeLabel::Accepted, delivered)
    } else {
        (
            JudgeLabel::Unknown,
            1.0 - delivered.max(abandoned).max(error),
        )
    }
}

fn label_of(reaction: Reaction, delivery: Delivery) -> JudgeLabel {
    match (reaction, delivery) {
        (Reaction::Corrected, _) => JudgeLabel::Corrected,
        (_, Delivery::None) => JudgeLabel::Abandoned,
        (_, Delivery::Shipped) => JudgeLabel::Shipped,
        (_, Delivery::Delivered) => JudgeLabel::Accepted,
        _ => JudgeLabel::Unknown,
    }
}

fn evidence(asked: &Asked) -> EvidenceQuality {
    if p(asked, CHECKED) >= CHECKED_AT {
        EvidenceQuality::Verified
    } else if p(asked, DELIVERED) >= YES {
        EvidenceQuality::Asserted
    } else {
        EvidenceQuality::Absent
    }
}

pub fn compose(asked: &Asked, shape: Shape) -> Option<Composed> {
    if asked.failed() {
        return None;
    }
    let delivery = delivery(asked);
    let evidence = evidence(asked);
    Some(match shape {
        Shape::Legacy => {
            let (label, confidence) = legacy(asked);
            Composed {
                label,
                reaction: Reaction::Unclear,
                delivery,
                confidence,
                evidence,
            }
        }
        Shape::TwoAxis { has_next } => {
            let (reaction, confidence) = reaction(asked, has_next);
            Composed {
                label: label_of(reaction, delivery),
                reaction,
                delivery,
                confidence,
                evidence,
            }
        }
    })
}
