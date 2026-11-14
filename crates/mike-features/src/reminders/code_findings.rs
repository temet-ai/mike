use bytes::Bytes;
use mike_base::code_notes::CodeNote;
use uuid::Uuid;

use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::store::models::ReminderSource;
use crate::wire::manifest::AppliedStage;

use super::{RemindersOutcome, try_append_reminder};

pub const HEADING: &str = "mike (the proxy) scanned the files your last edit changed and found code a rule flags (mike-code):";
pub const ASK: &str = "Fix it in this session, or say in one line why the code should stay.";
const PER_REQUEST: usize = 3;

pub fn note_text(notes: &[CodeNote]) -> String {
    let mut text = format!("{HEADING}\n");
    for note in notes {
        text.push_str(&format!(
            "- {}:{} {}: {}",
            note.path, note.line, note.rule_id, note.message
        ));
        if let Some(fix) = note.fix.as_deref().filter(|fix| !fix.is_empty()) {
            text.push_str(&format!(" Fix: `{fix}`"));
        }
        text.push('\n');
    }
    text.push_str(ASK);
    text
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
        let notes = ctx.state.code_notes.take(session, PER_REQUEST);
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
