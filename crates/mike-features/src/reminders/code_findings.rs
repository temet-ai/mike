use bytes::Bytes;
use mike_base::code_notes::CodeNote;
use uuid::Uuid;

use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::store::models::ReminderSource;
use crate::wire::manifest::AppliedStage;

use super::{RemindersOutcome, try_append_reminder};

pub const HEADING: &str = "mike checked the code your recent edits added. Fix these when you reach a stopping point; a fix needs no answer. Where the code is right as it is, put a comment on that line or the line above, `mike-keep: <rule> <reason>`, with the fact about the code that makes it so; it closes the finding wherever that line goes, and the reader of the diff sees it. Errors cannot be kept this way. `mike findings keep <number> --reason \"...\"` does the same without touching the file; weak reasons are refused. For a change that is not yours, run `mike findings not-mine <number>` and leave the file alone. One number covers its whole line.";
const GROUPS: usize = 8;
const LINES: usize = 5;

struct Group<'a> {
    first: &'a CodeNote,
    lines: Vec<i32>,
}

fn grouped(notes: &[CodeNote]) -> Vec<Group<'_>> {
    let mut groups: Vec<Group<'_>> = Vec::new();
    for note in notes {
        match groups
            .iter_mut()
            .find(|g| g.first.path == note.path && g.first.rule_id == note.rule_id)
        {
            Some(group) => group.lines.push(note.line),
            None => groups.push(Group {
                first: note,
                lines: vec![note.line],
            }),
        }
    }
    groups
}

fn group_line(group: &Group<'_>) -> String {
    let shown: Vec<String> = group
        .lines
        .iter()
        .take(LINES)
        .map(ToString::to_string)
        .collect();
    let count = group.lines.len();
    let more = if count > LINES {
        format!(" and {} more", count - LINES)
    } else {
        String::new()
    };
    let word = if count == 1 { "line" } else { "lines" };
    let first = group.first;
    let mut text = format!(
        "- [{}] {} {word} {}{more}, {}: {}",
        first.id,
        first.path,
        shown.join(", "),
        first.rule_id,
        first.message
    );
    if let Some(fix) = first.fix.as_deref().filter(|fix| !fix.is_empty()) {
        text.push_str(&format!(" Fix: `{fix}`"));
    }
    text
}

pub fn note_text(notes: &[CodeNote]) -> String {
    let groups = grouped(notes);
    let mut lines = vec![HEADING.to_owned()];
    lines.extend(groups.iter().take(GROUPS).map(group_line));
    let rest: usize = groups.iter().skip(GROUPS).map(|g| g.lines.len()).sum();
    if rest > 0 {
        lines.push(format!("- and {rest} more findings, not listed."));
    }
    lines.join("\n")
}

pub fn reminder_text(notes: &[CodeNote]) -> String {
    format!(
        "<system-reminder>\n{}\n</system-reminder>",
        note_text(notes)
    )
}

pub struct CodeFindingsStage;

#[async_trait::async_trait]
impl RequestStage for CodeFindingsStage {
    fn name(&self) -> &'static str {
        "code_findings"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        let toggles = ctx.state.feature_toggle_overlay.load();
        toggles.code_findings && toggles.code_scan && is_session_own_lane(ctx)
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let Ok(session) = ctx.session_id.parse::<Uuid>() else {
            body.record_unchanged("code_findings", "code_findings", "code_findings_no_session");
            return;
        };
        let notes = ctx.state.code_notes.take(session);
        if notes.is_empty() {
            body.record_unchanged(
                "code_findings",
                "code_findings",
                "code_findings_nothing_new",
            );
            return;
        }
        let ids: Vec<i64> = notes.iter().map(|n| n.id).collect();
        let outcome = try_append_reminder(
            body.body(),
            ReminderSource::Diagnostic,
            reminder_text(&notes),
            ids.clone(),
            0,
            ctx.request_id,
        );
        let RemindersOutcome::Appended {
            body: new_body,
            reminder_text,
            ..
        } = outcome
        else {
            ctx.state.code_notes.put_back(session, notes);
            body.record_unchanged(
                "code_findings",
                "code_findings",
                "code_findings_not_appended",
            );
            return;
        };
        ctx.state
            .code_notes
            .mark_sent(&ids, ctx.request_id.as_u64());
        let new_bytes = Bytes::from(new_body);
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "code_findings",
                policy_id: "code_findings",
                policy_version: 1,
                resolved_parameters: serde_json::json!({ "finding_ids": ids }),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "code_findings_injected",
                detail: Some(format!("injected {} bytes", reminder_text.len())),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: i64, path: &str, line: i32) -> CodeNote {
        CodeNote {
            id,
            path: path.to_owned(),
            line,
            rule_id: "sh-fixed-tmp-path".to_owned(),
            message: "A fixed path.".to_owned(),
            fix: None,
        }
    }

    #[test]
    fn findings_of_one_rule_in_one_file_are_one_line() {
        let notes: Vec<CodeNote> = (1..=12)
            .map(|n| note(n, "a.sh", i32::try_from(n).unwrap_or(0)))
            .collect();
        assert_eq!(
            note_text(&notes),
            format!(
                "{HEADING}\n- [1] a.sh lines 1, 2, 3, 4, 5 and 7 more, sh-fixed-tmp-path: A fixed path."
            )
        );
    }

    #[test]
    fn a_large_change_is_one_bounded_note() {
        let notes: Vec<CodeNote> = (1..=469).map(|n| note(n, &format!("f{n}.sh"), 1)).collect();
        let text = note_text(&notes);
        assert_eq!(text.lines().count(), GROUPS + 2);
        assert!(text.ends_with("- and 461 more findings, not listed."));
    }
}
