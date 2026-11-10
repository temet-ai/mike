use chrono::Duration;
use uuid::Uuid;

use super::*;

fn hit(reference: &str, session: Uuid, score: f64) -> Hit {
    Hit {
        reference: reference.to_owned(),
        session,
        session_title: "Docker images".to_owned(),
        date: "2026-10-21".to_owned(),
        excerpt: "lets get the keycloak changes ready".to_owned(),
        score,
    }
}

#[test]
fn points_at_past_with_last_week_is_true() {
    assert!(points_at_past(
        "we had a ticket WPCW-1657 that we worked on last week"
    ));
    assert!(points_at_past("I thought we fixed this"));
    assert!(points_at_past("why is discord gone again?"));
}

#[test]
fn points_at_past_with_a_present_tense_task_is_false() {
    assert!(!points_at_past("add a retry to the embed worker"));
    assert!(!points_at_past("rename the againstall helper"));
}

#[test]
fn operator_words_with_a_reminder_drops_the_reminder() {
    let text =
        "fix the build<system-reminder>Lesson recorded from earlier sessions</system-reminder> now";
    assert_eq!(operator_words(text), "fix the build now");
    assert!(!points_at_past(&operator_words(text)));
}

#[test]
fn operator_words_with_an_unclosed_reminder_drops_the_rest() {
    assert_eq!(operator_words("keep<system-reminder>yesterday"), "keep");
}

#[test]
fn window_with_yesterday_covers_the_last_three_days() {
    assert_eq!(
        window("the session yesterday on the compressor"),
        Duration::minutes(30)..Duration::days(3)
    );
}

#[test]
fn window_with_last_week_skips_today() {
    assert_eq!(
        window("we worked on it last week"),
        Duration::days(1)..Duration::days(15)
    );
}

#[test]
fn window_with_months_reaches_four_months_back() {
    assert_eq!(
        window("a month or two ago we had an issue"),
        Duration::days(7)..Duration::days(120)
    );
}

#[test]
fn window_without_a_time_word_covers_thirty_days() {
    assert_eq!(
        window("I thought we fixed this"),
        Duration::minutes(30)..Duration::days(30)
    );
}

#[test]
fn flatten_long_text_is_cut_with_an_ellipsis() {
    assert_eq!(flatten("one  two\nthree", 40), "one two three");
    assert_eq!(flatten("abcdefghij", 5), "abcd…");
}

#[test]
fn render_with_hits_lists_each_with_its_reference_and_marker() -> anyhow::Result<()> {
    let block = render(&[hit("turn:42412", Uuid::from_u128(2), 1.0)])
        .ok_or_else(|| anyhow::anyhow!("no block"))?;
    assert!(block.text.contains(EARLIER_MARKER_OPEN));
    assert!(block.text.contains("[turn:42412]"));
    assert!(
        block
            .text
            .contains("2026-10-21, session 00000000 \"Docker images\"")
    );
    assert!(block.text.contains("read_project_knowledge"));
    Ok(())
}

#[test]
fn render_with_no_hits_is_none() {
    assert_eq!(render(&[]), None);
}

#[test]
fn render_same_hits_twice_gives_the_same_hash() {
    let hits = [hit("turn:1", Uuid::from_u128(2), 1.0)];
    assert_eq!(render(&hits).map(|b| b.hash), render(&hits).map(|b| b.hash));
}
