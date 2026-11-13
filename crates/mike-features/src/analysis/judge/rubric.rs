use sha2::{Digest, Sha256};

pub const R3_ID: &str = "r3-2026-08-29";
pub const V2_ID: &str = "v2-2026-09-23";

pub use crate::settings::judge_prompts::R3_PROMPT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RubricFamily {
    LegacyOutcome,
    TwoAxis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rubric {
    pub id: &'static str,
    pub family: RubricFamily,
    pub system_prompt: &'static str,
}

pub const R3: Rubric = Rubric {
    id: R3_ID,
    family: RubricFamily::LegacyOutcome,
    system_prompt: R3_PROMPT,
};

pub const V2_2026_09_23: Rubric = Rubric {
    id: V2_ID,
    family: RubricFamily::TwoAxis,
    system_prompt: include_str!("rubrics/v2-2026-09-23.txt"),
};

pub const RUBRICS: &[Rubric] = &[R3, V2_2026_09_23];

pub fn rubric(id: &str) -> Option<&'static Rubric> {
    RUBRICS.iter().find(|r| r.id == id)
}

pub fn latest(family: RubricFamily) -> &'static Rubric {
    match family {
        RubricFamily::LegacyOutcome => &R3,
        RubricFamily::TwoAxis => &V2_2026_09_23,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveRubric {
    pub id: String,
    pub base: &'static Rubric,
    pub system_prompt: String,
}

pub fn sha12(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

pub fn effective_rubric(
    base: &'static Rubric,
    override_text: Option<&str>,
) -> EffectiveRubric {
    match override_text.filter(|t| !t.trim().is_empty() && *t != base.system_prompt) {
        Some(text) => EffectiveRubric {
            id: format!("{}+{}", base.id, sha12(text)),
            base,
            system_prompt: text.to_owned(),
        },
        None => EffectiveRubric {
            id: base.id.to_owned(),
            base,
            system_prompt: base.system_prompt.to_owned(),
        },
    }
}
