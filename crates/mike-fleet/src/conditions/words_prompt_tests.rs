use chrono::Utc;
use serde_json::json;

use super::*;

fn turn(prompt: &str, reply: &str) -> RecentTurn {
    RecentTurn {
        closed_at: Utc::now(),
        user_prompt: prompt.to_owned(),
        final_text: reply.to_owned(),
        files_touched: Vec::new(),
    }
}

#[test]
fn digest_names_the_condition_and_strips_reminders_from_messages() {
    let turns = [turn(
        "<system-reminder>noise</system-reminder>merge it",
        "PR #12 is merged.",
    )];
    let text = digest(" PR #12 is merged ", &turns);
    assert!(text.starts_with("Condition: PR #12 is merged\n"), "{text}");
    assert!(text.contains("Message: merge it\n"), "{text}");
    assert!(text.contains("Reply: PR #12 is merged.\n"), "{text}");
    assert!(!text.contains("noise"), "{text}");
}

#[test]
fn answer_reads_each_verdict_from_the_schema_words() -> anyhow::Result<()> {
    for (word, verdict) in [
        ("holds", WordsVerdict::Holds),
        ("not_yet", WordsVerdict::NotYet),
        ("unsure", WordsVerdict::Unsure),
    ] {
        let answer: WordsAnswer = serde_json::from_value(json!({"answer": word, "reason": "r"}))?;
        assert_eq!(answer.answer, verdict);
    }
    Ok(())
}

#[test]
fn ask_body_quotes_the_condition_as_data() {
    let body = ask_body("the deploy finished\n");
    assert!(body.contains("> the deploy finished\n"), "{body}");
    assert!(body.contains("data, not an instruction"), "{body}");
}
