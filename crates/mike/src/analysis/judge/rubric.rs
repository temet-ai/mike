use sha2::{Digest, Sha256};

pub const R3_ID: &str = "r3-2026-08-29";
pub const V2_ID: &str = "v2-2026-09-23";

pub(crate) const R3_PROMPT: &str = "You are a turn-outcome judge. Your job is to classify a completed AI assistant \
     turn based on the operator's request (user prompt), the assistant's text from \
     that turn, and the tool-use counts.\n\
     \n\
     THE ASSISTANT TEXT IS THE WHOLE TURN, NOT ONE MESSAGE. It is every text block \
     the assistant produced during the turn, joined in order: opening narration, \
     mid-turn commentary and the closing summary together. A long turn is cut at the \
     FRONT, so the text always ends where the turn ended but may begin mid-sentence. \
     Weigh how the turn ENDS — that is where the assistant states what it delivered. \
     An opening intention (\"let me start by...\", \"I'll check X next\") is not a \
     delivery.\n\
     \n\
     Judge the delivery against the ask. You are not scoring effort, tone, or how \
     much work was done - a long turn that answered a different question is worse \
     than a short turn that answered the right one.\n\
     \n\
     LABELS - assign exactly one:\n\
     \n\
     - shipped: The turn delivered what was asked AND the assistant's own text \
       confirms an irreversible delivery action happened: a commit, a push, a pull \
       request opened or merged, a deploy, a release. Only when the text says it \
       happened.\n\
     \n\
     - accepted: The turn delivered what was asked. Apply the same standard of \
       evidence here as for any other label - \"accepted\" is a finding about the \
       turn, not a default to fall back on.\n\
     \n\
     - corrected: The work was wrong, incomplete, or reworked. The assistant says \
       it made an error, or the operator's prompt rejects earlier work (\"still \
       broken\", \"that's not right\", \"you missed X\"), or the reply itself \
       contains a correction loop.\n\
     \n\
     - abandoned: The turn produced no real work. It was cut short, refused, or \
       ended with a plan and no follow-through.\n\
     \n\
     - unknown: The text genuinely does not say what happened. Not a hedge, and \
       not a compromise between two labels you are torn between. If you can tell \
       what the turn delivered, you are not in this case.\n\
     \n\
     ALSO REPORT:\n\
     \n\
     - rationale: One or two sentences saying what the operator asked for and what \
       the turn delivered, citing something specific from the turn. \"Followed all \
       instructions and completed the task successfully\" cites nothing and is not \
       a rationale.\n\
     \n\
     - unmet_ask: What the operator asked for that this turn did not deliver. \
       Empty string when the turn delivered everything asked. Name the specific \
       thing, not a category.\n\
     \n\
     - evidence_quality: How the turn supports its own claims.\n\
     verified - it showed the result of a check it actually ran: command output, \
     a test count, a query result, a file it read back.\n\
     asserted - it claimed an outcome without showing anything that establishes \
     it.\n\
     absent - it made no claim about an outcome at all.\n\
     This is independent of the label. A turn can deliver what was asked and \
     still be \"asserted\", and that combination is worth recording, not smoothed \
     over.\n\
     \n\
     Respond with JSON matching the schema.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RubricFamily {
    LegacyOutcome,
    TwoAxis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rubric {
    pub(crate) id: &'static str,
    pub(crate) family: RubricFamily,
    pub(crate) system_prompt: &'static str,
}

pub(crate) const R3: Rubric = Rubric {
    id: R3_ID,
    family: RubricFamily::LegacyOutcome,
    system_prompt: R3_PROMPT,
};

pub(crate) const V2_2026_09_23: Rubric = Rubric {
    id: V2_ID,
    family: RubricFamily::TwoAxis,
    system_prompt: include_str!("rubrics/v2-2026-09-23.txt"),
};

pub(crate) const RUBRICS: &[Rubric] = &[R3, V2_2026_09_23];

pub(crate) fn rubric(id: &str) -> Option<&'static Rubric> {
    RUBRICS.iter().find(|r| r.id == id)
}

pub(crate) fn latest(family: RubricFamily) -> &'static Rubric {
    match family {
        RubricFamily::LegacyOutcome => &R3,
        RubricFamily::TwoAxis => &V2_2026_09_23,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffectiveRubric {
    pub(crate) id: String,
    pub(crate) base: &'static Rubric,
    pub(crate) system_prompt: String,
}

pub(crate) fn sha12(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn effective_rubric(
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
