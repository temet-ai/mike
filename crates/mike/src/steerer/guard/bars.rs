use super::Question;

pub const RELAPSE_BAR: f64 = 0.85;
pub const UNBACKED_WAIT_BAR: f64 = 0.90;
pub const AUTHORISED_HANDBACK_BAR: f64 = 0.80;
pub const UNOBSERVED_DONE_BAR: f64 = 0.80;

pub fn promotion_bar(question: Question) -> f64 {
    match question {
        Question::Relapse => RELAPSE_BAR,
        Question::UnbackedWait => UNBACKED_WAIT_BAR,
        Question::AuthorisedHandback => AUTHORISED_HANDBACK_BAR,
        Question::UnobservedDone => UNOBSERVED_DONE_BAR,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct GateBar {
    pub question: Question,
    pub targets: &'static [&'static str],
    pub min_n: i64,
    pub min_precision: f64,
    pub min_wilson_low: Option<f64>,
}

pub fn gate_bar(question: Question) -> GateBar {
    match question {
        Question::Relapse => GateBar {
            question,
            targets: &["append", "wake"],
            min_n: 40,
            min_precision: RELAPSE_BAR,
            min_wilson_low: Some(0.75),
        },
        Question::UnbackedWait => GateBar {
            question,
            targets: &["append", "wake"],
            min_n: 50,
            min_precision: UNBACKED_WAIT_BAR,
            min_wilson_low: None,
        },
        Question::AuthorisedHandback => GateBar {
            question,
            targets: &["append"],
            min_n: 30,
            min_precision: AUTHORISED_HANDBACK_BAR,
            min_wilson_low: None,
        },
        Question::UnobservedDone => GateBar {
            question,
            targets: &["append"],
            min_n: 40,
            min_precision: UNOBSERVED_DONE_BAR,
            min_wilson_low: None,
        },
    }
}
