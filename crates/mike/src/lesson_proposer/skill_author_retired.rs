use std::fmt::Write as _;

use super::AuthoredReference;
use crate::store::persistence::lessons::skill_live::{RetiredNumbers, RetiredReason, RetiredSkill};

fn rate(part: i32, whole: i32) -> String {
    if whole <= 0 {
        return "not measurable (no judged turn)".to_owned();
    }
    format!(
        "{:.0}% ({part} of {whole})",
        f64::from(part) / f64::from(whole) * 100.0
    )
}

fn why(numbers: &RetiredNumbers) -> String {
    match numbers.reason {
        RetiredReason::Unloaded => format!(
            "No session loaded it for 30 days after it shipped on {}; its description did not match the work people did.",
            numbers.first_shipped_on
        ),
        RetiredReason::CorrectedAboveBase => "Sessions that loaded it were corrected more often \
             than the project's turns in general, so loading it did not help the work it was \
             written for."
            .to_owned(),
    }
}

fn measured(numbers: &RetiredNumbers) -> String {
    format!(
        "Shipped on {}. From {} to {} it was loaded {} times in {} operator turns. \
         Of the judged ones, the judge marked corrected {}. \
         Across all of the project's judged turns in the same window the corrected rate was {}.\n",
        numbers.first_shipped_on,
        numbers.window_start.date_naive(),
        numbers.window_end.date_naive(),
        numbers.loads,
        numbers.loaded_turns,
        rate(numbers.corrected_turns, numbers.judged_turns),
        rate(numbers.base_corrected_turns, numbers.base_judged_turns)
    )
}

pub(super) fn section(retired: &RetiredSkill) -> String {
    let mut out = format!(
        "## RETIRED SKILL\nThe skill for this topic was shipped and then retired by its live use. \
         Write a different skill, not a copy of it.\n\n### What happened\n{}\n{}\n",
        why(&retired.numbers),
        measured(&retired.numbers)
    );
    let Some(shipped) = &retired.shipped else {
        out.push_str(
            "The retired skill was generated from the lessons without an authored draft.\n",
        );
        return out;
    };
    let _ = write!(
        out,
        "### Its description\n{}\n\n### Its SKILL.md\n{}\n",
        shipped.description.trim(),
        shipped.skill_md.trim()
    );
    let references: Vec<AuthoredReference> =
        serde_json::from_value(shipped.reference_files.clone()).unwrap_or_default();
    for reference in &references {
        let _ = write!(
            out,
            "\n### references/{}.md\n{}\n",
            reference.name,
            reference.content.trim()
        );
    }
    out
}
