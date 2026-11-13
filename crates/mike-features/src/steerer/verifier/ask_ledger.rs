use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;
use crate::store::persistence::steering::PriorFireRow;

pub fn classify_ask_kind(is_first: bool, text: &str) -> &'static str {
    if is_first {
        return "initial";
    }
    if crate::wire::machine_text::classify(text).is_some() {
        return "clarification";
    }
    const CORRECTION_MARKERS: &[&str] = &[
        "actually,",
        "actually ",
        "no, ",
        "no—",
        "no--",
        "that's wrong",
        "that's not",
        "not what i",
        "not what you",
        "you didn't",
        "you missed",
        "wrong,",
        "instead of",
        "don't do",
        "stop doing",
    ];
    let lower = text.to_lowercase();
    if CORRECTION_MARKERS.iter().any(|m| lower.contains(m)) {
        "correction"
    } else {
        "clarification"
    }
}

pub fn is_operator_ask(text: &str) -> bool {
    crate::wire::machine_text::classify(text).is_none()
        && !crate::wire::genuine::is_tool_prompt(text)
}

pub fn written_by_the_operator(entries: Vec<AskLedgerEntryRow>) -> Vec<AskLedgerEntryRow> {
    entries
        .into_iter()
        .filter(|entry| is_operator_ask(&entry.content))
        .map(|mut entry| {
            entry.content = crate::wire::genuine::operator_words(&entry.content);
            entry
        })
        .collect()
}

pub fn build_accumulated_ask(entries: &[AskLedgerEntryRow]) -> String {
    entries
        .iter()
        .map(|e| format!("[{}] {}", e.kind, e.content))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn build_prior_fires_digest(fires: &[PriorFireRow]) -> String {
    if fires.is_empty() {
        return String::new();
    }
    fires
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let reason = f
                .off_thread_topic
                .as_deref()
                .unwrap_or("(no reason recorded)");
            format!(
                "{}. {} — {reason}",
                i + 1,
                f.created_at.format("%Y-%m-%d %H:%M:%SZ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn classify_ask_kind_first_entry_is_initial_regardless_of_wording() {
        assert_eq!(
            classify_ask_kind(true, "actually, no, that's wrong"),
            "initial"
        );
    }

    #[test]
    fn classify_ask_kind_correction_marker_detected() {
        assert_eq!(
            classify_ask_kind(false, "actually, that's not what I asked for"),
            "correction"
        );
        assert_eq!(
            classify_ask_kind(false, "no, stop doing that"),
            "correction"
        );
    }

    #[test]
    fn classify_ask_kind_teammate_relayed_report_never_correction() {
        let text = "Another Claude session sent a message:\n\
                     <teammate-message teammate_id=\"verify-121\" color=\"blue\" summary=\"Full #121 verdict\">\n\
                     I pulled the actual tool-call transcript for the commit's own session — \
                     request 242376: curl against the **actually running proxy** → HTTP_STATUS:200.\n\
                     </teammate-message>";
        assert_ne!(classify_ask_kind(false, text), "correction");
    }

    #[test]
    fn classify_ask_kind_any_machine_authored_text_never_correction() {
        let teammate_26 = "<teammate-message teammate_id=\"team-lead\" summary=\"Verify #121 resolved-as-fixed claim\">\n\
                            Independent acceptance verification. Repo: /Users/vimes/projects/temet/temet-proxy (branch main).\n\
                            </teammate-message>";
        let teammate_27 = "<teammate-message teammate_id=\"team-lead\" summary=\"Verify #124-DB slice (029b7eed)\">\n\
                            Independent acceptance verification. Repo: /Users/vimes/projects/temet/temet-proxy (branch main).\n\
                            </teammate-message>";
        let hook_38 = "Stop hook feedback:\nyou have 1 unacknowledged cross-session message(s); handle them, \
                        then acknowledge: curl -X POST http://127.0.0.1:9989/api/sessions/x/messages/y/ack";
        let wake_42 = "Fleet coordinator cycle wake (steerer-firing-model successor run): (1) heartbeat + inbox; \
                        (2) poll worker heads rust-a ac524070 / rust-b 5851e710 / observer ca7a3985 via get_session_head.";
        let hook_with_marker = "Stop hook feedback:\nactually, you missed an ack — handle it.";

        for specimen in [teammate_26, teammate_27, hook_38, wake_42, hook_with_marker] {
            assert_ne!(
                classify_ask_kind(false, specimen),
                "correction",
                "machine-authored text must not arm #114: {specimen}"
            );
        }
    }

    fn ledger_entry(id: i64, content: &str) -> AskLedgerEntryRow {
        AskLedgerEntryRow {
            id,
            session_id: 1,
            operator_turn_id: None,
            kind: "clarification".to_owned(),
            content: content.to_owned(),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn asks_the_operator_did_not_write_are_left_out_of_what_the_judge_reads() {
        let entries = vec![
            ledger_entry(1, "build the importer"),
            ledger_entry(
                2,
                "Respond with TEXT ONLY. Do NOT call any tools.\n\nYour task is to create a detailed summary of the conversation so far.",
            ),
            ledger_entry(
                3,
                "<task-notification>\n<status>completed</status>\n</task-notification>",
            ),
            ledger_entry(4, "also add a test for the edge case"),
        ];
        let kept: Vec<i64> = written_by_the_operator(entries)
            .iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(kept, vec![1, 4]);
        assert!(is_operator_ask("please summarise what changed today"));
    }

    #[test]
    fn an_ask_sent_mid_turn_is_read_without_claude_codes_wording_around_it() {
        let stored = "The user sent a new message while you were working:\nboundary detection is severely lacking\n\nThis is how Claude Code surfaces messages the user sends mid-turn — within the running turn, often alongside the next tool result, rather than as a separate conversation turn. Address the message above as you continue this turn.";
        let read = written_by_the_operator(vec![ledger_entry(7, stored)]);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].content, "boundary detection is severely lacking");
    }

    #[test]
    fn classify_ask_kind_defaults_to_clarification() {
        assert_eq!(
            classify_ask_kind(false, "also add a test for the edge case"),
            "clarification"
        );
    }

    fn make_ledger_entry(kind: &str, content: &str) -> AskLedgerEntryRow {
        AskLedgerEntryRow {
            id: 1,
            session_id: 1,
            operator_turn_id: None,
            kind: kind.to_string(),
            content: content.to_string(),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn build_accumulated_ask_joins_oldest_first_with_kind_labels() {
        let entries = vec![
            make_ledger_entry("initial", "build the ask ledger"),
            make_ledger_entry("clarification", "just the schema half"),
        ];
        let ask = build_accumulated_ask(&entries);
        assert_eq!(
            ask,
            "[initial] build the ask ledger\n[clarification] just the schema half"
        );
    }

    #[test]
    fn build_accumulated_ask_empty_for_no_entries() {
        assert_eq!(build_accumulated_ask(&[]), "");
    }

    fn make_prior_fire(off_thread_topic: Option<&str>) -> PriorFireRow {
        PriorFireRow {
            off_thread_topic: off_thread_topic.map(str::to_string),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn build_prior_fires_digest_empty_for_no_fires() {
        assert_eq!(build_prior_fires_digest(&[]), "");
    }

    #[test]
    fn build_prior_fires_digest_numbers_and_carries_reason() {
        let fires = vec![
            make_prior_fire(Some("claimed tests pass with no test command run")),
            make_prior_fire(None),
        ];
        let summary = build_prior_fires_digest(&fires);
        assert!(summary.contains("1. "));
        assert!(summary.contains("claimed tests pass with no test command run"));
        assert!(summary.contains("2. "));
        assert!(summary.contains("(no reason recorded)"));
    }
}
