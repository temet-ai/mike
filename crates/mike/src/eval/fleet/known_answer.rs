use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct KnownAnswer {
    pub relanded: bool,
    pub test_passed: bool,
}

impl KnownAnswer {
    pub fn label(&self) -> &'static str {
        if self.relanded && self.test_passed {
            "accepted"
        } else {
            "corrected"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct JudgeAccuracy {
    pub judged: usize,
    pub agreed: usize,
    pub accuracy: Option<f64>,
}

pub fn judge_accuracy(verdicts: &[(KnownAnswer, &str)]) -> JudgeAccuracy {
    let agreed = verdicts
        .iter()
        .filter(|(known, verdict)| known.label() == *verdict)
        .count();
    JudgeAccuracy {
        judged: verdicts.len(),
        agreed,
        accuracy: (!verdicts.is_empty()).then(|| agreed as f64 / verdicts.len() as f64),
    }
}
