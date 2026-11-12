pub(crate) fn strip_harness_scaffold(raw: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        match rest[start..].find(CLOSE) {
            Some(end) => rest = &rest[start + end + CLOSE.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    crate::wire::genuine::operator_words(out.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_scaffold_recovers_genuine_ask() {
        let raw = "<system-reminder>\nhuge claudeMd context...\n</system-reminder>\n\nCan you explain how photosynthesis works?";
        assert_eq!(
            strip_harness_scaffold(raw),
            "Can you explain how photosynthesis works?"
        );
    }

    #[test]
    fn a_message_sent_mid_turn_is_reduced_to_the_operators_words() {
        let raw = "<system-reminder>context</system-reminder>\nThe user sent a new message while you were working:\ncheck the holdfast sessions\n\nThis is how Claude Code surfaces messages the user sends mid-turn — within the running turn, often alongside the next tool result, rather than as a separate conversation turn. Address the message above as you continue this turn.";
        assert_eq!(strip_harness_scaffold(raw), "check the holdfast sessions");
    }

    #[test]
    fn strip_scaffold_removes_multiple_reminders() {
        let raw =
            "<system-reminder>a</system-reminder>real text<system-reminder>b</system-reminder>";
        assert_eq!(strip_harness_scaffold(raw), "real text");
    }

    #[test]
    fn strip_scaffold_pure_scaffold_is_empty() {
        let raw = "<system-reminder>only context, no user ask</system-reminder>";
        assert_eq!(strip_harness_scaffold(raw), "");
    }

    #[test]
    fn strip_scaffold_plain_message_unchanged() {
        assert_eq!(strip_harness_scaffold("just a question"), "just a question");
    }
}
