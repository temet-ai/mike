use std::fmt::Write as _;

use super::AuthoredReference;
use crate::lesson_proposer::skill_findings::describe_eval;
use crate::store::persistence::lessons::skill_drafts::PreviousDraft;

pub fn section(previous: &PreviousDraft) -> String {
    let mut out = format!(
        "## PREVIOUS DRAFT (REJECTED)\nDraft {} for this topic was evaluated and rejected. \
         Rewrite the description and the reference files so that what failed below passes.\n\n\
         ### Its description\n{}\n\n### Its SKILL.md\n{}\n",
        previous.revision,
        previous.description.trim(),
        previous.skill_md.trim()
    );
    let references: Vec<AuthoredReference> =
        serde_json::from_value(previous.reference_files.clone()).unwrap_or_default();
    for reference in &references {
        let _ = write!(
            out,
            "\n### references/{}.md\n{}\n",
            reference.name,
            reference.content.trim()
        );
    }
    let evaluation = previous
        .eval
        .as_ref()
        .map(describe_eval)
        .unwrap_or_default();
    let _ = write!(
        out,
        "\n### Its evaluation\n{}\n",
        if evaluation.trim().is_empty() {
            "No findings were recorded.\n"
        } else {
            evaluation.as_str()
        }
    );
    out
}
