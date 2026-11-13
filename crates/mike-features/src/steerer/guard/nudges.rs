use super::Question;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NudgeInputs<'a> {
    pub correction_text: &'a str,
    pub evidence_quote: &'a str,
    pub ending_quote: &'a str,
    pub open_items: usize,
    pub awaited_replies: usize,
    pub last_change_tool: &'a str,
    pub last_change_at: &'a str,
    pub authorisation_class: &'a str,
    pub authorisation_text: &'a str,
    pub authorised_at: &'a str,
}

pub fn render_nudge(question: Question, i: &NudgeInputs<'_>) -> String {
    match question {
        Question::Relapse => format!(
            "Earlier in this session the operator corrected you. Their words:\n\n{}\n\n(End of the operator's words.)\n\nThis turn looks like the same thing again: {}\n\nIf it is, apply the correction now. If something stops you, end with `Blocked:` and say what.",
            i.correction_text, i.evidence_quote
        ),
        Question::UnbackedWait => format!(
            "You ended on: \"{}\". I do not see anything running that would bring you back (work registry: {} open items, {} awaited replies). If something is, say what. Otherwise do it now, or end with `Blocked:` naming what you are waiting for.",
            i.ending_quote, i.open_items, i.awaited_replies
        ),
        Question::UnobservedDone => format!(
            "You reported this as done, and I do not see anything after the last change ({} at {}) that observed the result. If you did check it, say how. Otherwise run it, render it or test it, or say it is unverified.",
            i.last_change_tool, i.last_change_at
        ),
        Question::AuthorisedHandback => format!(
            "You were already authorised to {}: \"{}\" ({}). Go ahead without asking.",
            i.authorisation_class, i.authorisation_text, i.authorised_at
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relapse_nudge_quotes_the_correction_byte_for_byte() {
        let correction = "stop waiting on CI.\n  > run the tests yourself\n\tthen report";
        let text = render_nudge(
            Question::Relapse,
            &NudgeInputs {
                correction_text: correction,
                evidence_quote: "I'll report when CI lands.",
                ..NudgeInputs::default()
            },
        );
        assert!(text.contains(correction));
        assert!(text.contains("(End of the operator's words.)"));
    }

    #[test]
    fn wait_nudge_names_the_ending_and_the_registry() {
        let text = render_nudge(
            Question::UnbackedWait,
            &NudgeInputs {
                ending_quote: "I'll check back soon",
                ..NudgeInputs::default()
            },
        );
        assert!(text.contains("\"I'll check back soon\""));
        assert!(text.contains("0 open items, 0 awaited replies"));
    }

    #[test]
    fn the_fixed_notes_leave_room_for_the_work_being_done_already() {
        let done = render_nudge(
            Question::UnobservedDone,
            &NudgeInputs {
                last_change_tool: "tool 806624",
                last_change_at: "22:10:00",
                ..NudgeInputs::default()
            },
        );
        assert!(done.contains("(tool 806624 at 22:10:00)"));
        assert!(done.contains("If you did check it, say how."));
        let wait = render_nudge(Question::UnbackedWait, &NudgeInputs::default());
        assert!(wait.contains("If something is, say what."));
        for text in [done, wait] {
            assert!(!text.contains("say plainly"));
        }
    }
}
