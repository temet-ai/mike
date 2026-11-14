use super::input_v2::{INPUT_BUILDER_V2_SLICES, JudgeInput};
use crate::dossier::render::judge::JudgeSectionId;
use crate::store::persistence::turn_facts::TurnFactsRow;

pub const ADDENDUM_ID: &str = "context-addendum-2026-09-24";
pub const CONTEXT_ADDENDUM: &str = include_str!("rubrics/context-addendum-2026-09-24.txt");

const NEXT_MARKER: &str = "OPERATOR'S NEXT GENUINE MESSAGE (automated events removed):\n";
const TURNFACTS_HEADING: &str =
    "FACTS ABOUT THIS TURN (recorded by the proxy, not written by the assistant):";
const NONE: &str = "(none)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgeSlice {
    Asks,
    Directives,
    Corrections,
    Turnfacts,
    WithholdNext,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error(
    "unknown history slice `{0}`; expected asks, directives, corrections, turnfacts or withhold_next"
)]
pub struct UnknownSliceCode(pub String);

impl JudgeSlice {
    pub const APPENDED: [JudgeSlice; 4] = [
        JudgeSlice::Asks,
        JudgeSlice::Directives,
        JudgeSlice::Corrections,
        JudgeSlice::Turnfacts,
    ];

    pub fn from_code(code: &str) -> Result<Self, UnknownSliceCode> {
        match code {
            "asks" => Ok(Self::Asks),
            "directives" => Ok(Self::Directives),
            "corrections" => Ok(Self::Corrections),
            "turnfacts" => Ok(Self::Turnfacts),
            "withhold_next" => Ok(Self::WithholdNext),
            other => Err(UnknownSliceCode(other.to_owned())),
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Asks => "asks",
            Self::Directives => "directives",
            Self::Corrections => "corrections",
            Self::Turnfacts => "turnfacts",
            Self::WithholdNext => "withhold_next",
        }
    }

    fn heading(self) -> Option<&'static str> {
        match self {
            Self::Asks => Some(JudgeSectionId::EarlierOperatorMessages.heading()),
            Self::Directives => Some(JudgeSectionId::Directives.heading()),
            Self::Corrections => Some(JudgeSectionId::PriorCorrections.heading()),
            Self::Turnfacts => Some(TURNFACTS_HEADING),
            Self::WithholdNext => None,
        }
    }
}

pub fn parse_slice_codes<S: AsRef<str>>(codes: &[S]) -> Result<Vec<JudgeSlice>, UnknownSliceCode> {
    codes
        .iter()
        .map(|code| JudgeSlice::from_code(code.as_ref()))
        .collect()
}

pub fn slice_codes(slices: &[JudgeSlice]) -> Vec<String> {
    slices.iter().map(|s| s.code().to_owned()).collect()
}

pub fn needs_addendum(slices: &[JudgeSlice]) -> bool {
    slices.iter().any(|s| *s != JudgeSlice::WithholdNext)
}

pub fn compose_system_prompt(base: &str, slices: &[JudgeSlice]) -> String {
    if needs_addendum(slices) {
        format!("{base}\n\n{CONTEXT_ADDENDUM}")
    } else {
        base.to_owned()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SliceTexts {
    pub asks: Option<String>,
    pub directives: Option<String>,
    pub corrections: Option<String>,
    pub turnfacts: Option<String>,
}

impl SliceTexts {
    fn body(&self, slice: JudgeSlice) -> &str {
        let text = match slice {
            JudgeSlice::Asks => &self.asks,
            JudgeSlice::Directives => &self.directives,
            JudgeSlice::Corrections => &self.corrections,
            JudgeSlice::Turnfacts => &self.turnfacts,
            JudgeSlice::WithholdNext => &None,
        };
        text.as_deref().filter(|t| !t.is_empty()).unwrap_or(NONE)
    }
}

fn withhold_next(mut text: String) -> String {
    if let Some(at) = text.rfind(NEXT_MARKER) {
        text.truncate(at + NEXT_MARKER.len());
        text.push_str(NONE);
    }
    text
}

pub fn apply_slices(base: JudgeInput, slices: &[JudgeSlice], texts: &SliceTexts) -> JudgeInput {
    let withheld = slices.contains(&JudgeSlice::WithholdNext);
    let mut text = if withheld {
        withhold_next(base.text)
    } else {
        base.text
    };
    let mut appended = false;
    for slice in JudgeSlice::APPENDED
        .into_iter()
        .filter(|s| slices.contains(s))
    {
        let Some(heading) = slice.heading() else {
            continue;
        };
        text.push_str("\n\n");
        text.push_str(heading);
        text.push('\n');
        text.push_str(texts.body(slice));
        appended = true;
    }
    JudgeInput {
        turn_id: base.turn_id,
        text,
        builder_version: if appended {
            INPUT_BUILDER_V2_SLICES
        } else {
            base.builder_version
        },
        as_of: base.as_of,
        next_genuine_request_id: base.next_genuine_request_id,
    }
}

fn checks_line(count: i32, failed: i32) -> String {
    if count <= 0 {
        "none".to_owned()
    } else {
        format!("{count}, {failed} returned an error")
    }
}

fn yes_no(flag: bool) -> &'static str {
    if flag { "yes" } else { "no" }
}

fn or_unknown(text: Option<&str>) -> &str {
    text.filter(|t| !t.is_empty()).unwrap_or("unknown")
}

pub fn render_turnfacts(row: Option<&TurnFactsRow>) -> String {
    let Some(row) = row else {
        return NONE.to_owned();
    };
    format!(
        "Stop reason of the turn's last main-lane response: {}\n\
         Interrupted (a request in the turn was aborted): {}\n\
         Checks run (test, build, lint, curl or screenshot commands): {}\n\
         How the turn ends, by phrase class: {}",
        or_unknown(row.stop_reason.as_deref()),
        yes_no(row.aborted || row.interrupted),
        checks_line(row.checks_count, row.checks_failed),
        or_unknown(Some(&row.ended_on)),
    )
}
