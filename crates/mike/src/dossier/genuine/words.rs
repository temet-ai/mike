use super::markers::{MID_TURN_LEAD_IN, MID_TURN_NOTICE};

pub fn operator_words(text: &str) -> String {
    if !text.trim_start().starts_with(MID_TURN_LEAD_IN) {
        return text.to_owned();
    }
    text.lines()
        .filter(|line| {
            let line = line.trim();
            line != MID_TURN_LEAD_IN && !line.starts_with(MID_TURN_NOTICE)
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENTENCE: &str = "This is how Claude Code surfaces messages the user sends mid-turn — within the running turn, often alongside the next tool result, rather than as a separate conversation turn. Address the message above as you continue this turn.";

    fn sent_mid_turn(words: &str) -> String {
        format!("{MID_TURN_LEAD_IN}\n{words}\n\n{SENTENCE}")
    }

    #[test]
    fn a_message_sent_mid_turn_is_reduced_to_what_the_operator_typed() {
        assert_eq!(
            operator_words(&sent_mid_turn(
                "there are new holdfast sessions\nyou can check"
            )),
            "there are new holdfast sessions\nyou can check"
        );
    }

    #[test]
    fn two_messages_sent_before_the_next_request_keep_both_and_lose_both_wrappers() {
        let both = format!("{}\n{}", sent_mid_turn("first"), sent_mid_turn("second"));
        assert_eq!(operator_words(&both), "first\n\nsecond");
    }

    #[test]
    fn a_message_sent_between_turns_is_returned_as_it_is() {
        let typed = "please add a retry\n\nand keep the old behaviour";
        assert_eq!(operator_words(typed), typed);
        let quoting = format!("why does every message end with this?\n{SENTENCE}");
        assert_eq!(operator_words(&quoting), quoting);
    }
}
