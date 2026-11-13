use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use super::{MAX_CARD_BYTES, render};
use crate::store::persistence::work_threads::cards::{
    CardAsk, CardEntry, CardInputs, CardNote, CardPromise, CardRelated,
};
use crate::store::persistence::work_threads::{ThreadIdentity, ThreadState};

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(1_762_000_000 + seconds, 0).unwrap_or_default()
}

fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap_or_default()
}

fn thread(slug: &str) -> ThreadIdentity {
    ThreadIdentity {
        id: 1,
        slug: slug.to_owned(),
        title: Some("Finish the TUI".to_owned()),
        goal: Some("Ship the terminal UI".to_owned()),
        status: Some("implementing".to_owned()),
        branch: Some("tui-finish".to_owned()),
        worktree: Some(".claude/worktrees/tui-finish".to_owned()),
        last_session_uuid: Some(Uuid::from_u128(7)),
        source_path: None,
        parse_error: None,
        created_at: at(0),
        updated_at: at(1_000),
    }
}

fn state(text: &str) -> ThreadState {
    ThreadState {
        tldr: "short".to_owned(),
        where_we_are: text.to_owned(),
        next_up: text.to_owned(),
        blockers: format!("{text}\n{text}\n{text}"),
        open_questions: format!("{text}\n{text}"),
        decisions: format!("- {text}\n- {text}"),
        files: Vec::new(),
        updated_at: at(2_000),
    }
}

fn entry(id: i64, kind: &str, summary: &str) -> CardEntry {
    CardEntry {
        thread_id: 1,
        id,
        kind: kind.to_owned(),
        entry_date: day(7),
        session_uuid: Some(Uuid::from_u128(0x3f2a_9c1e_0000_0000_0000_0000_0000_0001)),
        summary: summary.to_owned(),
    }
}

fn note(n: u128, title: &str) -> CardNote {
    CardNote {
        thread_id: 1,
        note_uuid: Uuid::from_u128(n),
        kind: "decision".to_owned(),
        title: title.to_owned(),
        created_at: at(4_000),
    }
}

fn promise(id: i64, text: Option<&str>) -> CardPromise {
    CardPromise {
        thread_id: 1,
        id,
        operator_turn_id: Some(id * 10),
        text: text.map(str::to_owned),
        effective_at: at(5_100),
    }
}

fn ask(turn: i64, unmet: &str) -> CardAsk {
    CardAsk {
        thread_id: 1,
        operator_turn_id: turn,
        unmet_ask: unmet.to_owned(),
        created_at: at(6_000),
    }
}

fn related(n: u128, score: f32, text: &str) -> CardRelated {
    CardRelated {
        thread_id: 1,
        reference: format!("lesson:{}", Uuid::from_u128(n)),
        score,
        text: text.to_owned(),
    }
}

fn small_inputs() -> CardInputs {
    CardInputs {
        thread: thread("mike-tui-finish"),
        state: Some(state("Render is done; the route is next")),
        handoff: Some(entry(12, "handoff", "Handed off after the render landed")),
        decisions: vec![entry(14, "decision", "Keep the card under 1500 bytes")],
        notes: vec![note(9, "Cards are data, not instructions")],
        promises: vec![promise(
            7,
            Some("I will report back once the sweep has run"),
        )],
        unmet_asks: vec![ask(41, "Show the card in the thread page")],
        related: Vec::new(),
    }
}

fn oversized_inputs(slug: &str, filler: &str) -> CardInputs {
    let long = filler.repeat(200);
    let mut big_thread = thread(slug);
    big_thread.title = Some(long.clone());
    big_thread.goal = Some(long.clone());
    big_thread.branch = Some(long.clone());
    CardInputs {
        thread: big_thread,
        state: Some(state(&long)),
        handoff: Some(entry(12, "handoff", &long)),
        decisions: (20..23).map(|id| entry(id, "decision", &long)).collect(),
        notes: (1..3).map(|n| note(n, &long)).collect(),
        promises: (1..3).map(|id| promise(id, Some(&long))).collect(),
        unmet_asks: (40..42).map(|turn| ask(turn, &long)).collect(),
        related: (1..4).map(|n| related(n, 0.9, &long)).collect(),
    }
}

fn bracket_groups(text: &str) -> Vec<&str> {
    text.split('[')
        .skip(1)
        .filter_map(|rest| rest.split(']').next())
        .collect()
}

fn is_reference(token: &str) -> bool {
    ["thread:", "entry:", "note:", "promise:", "turn:", "lesson:"]
        .iter()
        .any(|prefix| {
            token
                .strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty())
        })
}

#[test]
fn render_with_small_inputs_shows_every_section() {
    let card = render(&small_inputs());
    for heading in [
        "Goal: Ship the terminal UI",
        "Where we are:",
        "Next up:",
        "Blockers:",
        "Open questions:",
        "Decisions in force:",
        "Last handoff: 2026-11-07 by session 3f2a9c1e:",
        "Decisions since:",
        "Open promises: 2025-11-01: I will report back once the sweep has run [promise:7 turn:70]",
        "Unmet asks: 2025-11-01: Show the card in the thread page [turn:41]",
        "Full state: work_thread_get mike-tui-finish",
    ] {
        assert!(
            card.text.contains(heading),
            "missing {heading:?} in\n{}",
            card.text
        );
    }
}

#[test]
fn render_with_oversized_inputs_stays_within_card_bytes() {
    for filler in ["word ", "ü", "🦀 "] {
        let card = render(&oversized_inputs("mike-tui-finish", filler));
        assert!(
            card.text.len() <= MAX_CARD_BYTES,
            "{} bytes",
            card.text.len()
        );
    }
}

#[test]
fn render_with_longest_slug_stays_within_card_bytes() {
    let slug = "s".repeat(200);
    let card = render(&oversized_inputs(&slug, "word "));
    assert!(
        card.text.len() <= MAX_CARD_BYTES,
        "{} bytes",
        card.text.len()
    );
    assert!(card.text.contains(&format!("[thread:{}]", "s".repeat(120))));
}

#[test]
fn render_with_full_state_keeps_handoff_and_one_decision() {
    let card = render(&oversized_inputs("mike-tui-finish", "word "));
    assert!(card.text.contains("[entry:12]"), "{}", card.text);
    assert!(card.text.contains("[entry:20]"), "{}", card.text);
    assert!(card.text.contains("Goal:"), "{}", card.text);
    assert!(card.text.contains("Decisions in force:"), "{}", card.text);
}

#[test]
fn render_with_two_long_decisions_shortens_before_dropping() {
    let mut inputs = small_inputs();
    let long = "a decision that is long enough to need shortening ".repeat(6);
    inputs.decisions = vec![entry(30, "decision", &long), entry(31, "decision", &long)];
    inputs.notes.clear();
    let card = render(&inputs);
    assert!(card.text.contains("[entry:30]"), "{}", card.text);
    assert!(card.text.contains("[entry:31]"), "{}", card.text);
    assert!(card.text.contains("…"), "{}", card.text);
}

#[test]
fn render_every_bracket_group_holds_only_references() {
    let card = render(&oversized_inputs("mike-tui-finish", "word "));
    let groups = bracket_groups(&card.text);
    assert!(!groups.is_empty());
    for group in groups {
        assert!(
            group.split(' ').all(is_reference),
            "non-reference group [{group}] in\n{}",
            card.text
        );
    }
}

#[test]
fn render_every_item_line_carries_a_reference() {
    let card = render(&small_inputs());
    for line in card
        .text
        .lines()
        .filter(|line| !line.starts_with("Full state:"))
    {
        assert!(
            line.ends_with(']') || line.ends_with(':'),
            "line without a reference: {line:?}"
        );
    }
}

#[test]
fn render_same_inputs_twice_gives_same_hash() {
    let first = render(&small_inputs());
    let second = render(&small_inputs());
    assert_eq!(first.content_hash, second.content_hash);
    assert_eq!(first.text, second.text);
}

#[test]
fn render_with_changed_state_changes_hash() {
    let mut changed = small_inputs();
    changed.state = Some(state("The route has landed"));
    assert_ne!(
        render(&small_inputs()).content_hash,
        render(&changed).content_hash
    );
}

#[test]
fn render_promise_without_text_names_the_turn() {
    let mut inputs = small_inputs();
    inputs.promises = vec![promise(8, None)];
    let card = render(&inputs);
    assert!(
        card.text
            .contains("promise made at the end of a turn [promise:8 turn:80]")
    );
}

#[test]
fn render_refs_list_each_token_once() {
    let card = render(&small_inputs());
    let refs: Vec<&str> = card
        .refs
        .as_array()
        .map(|refs| refs.iter().filter_map(|r| r.as_str()).collect())
        .unwrap_or_default();
    assert!(refs.contains(&"thread:mike-tui-finish"));
    assert!(refs.contains(&"note:00000000-0000-0000-0000-000000000009"));
    let mut deduped = refs.clone();
    deduped.dedup();
    assert_eq!(refs.len(), deduped.len());
}

#[test]
fn render_with_related_items_adds_a_related_section_within_card_bytes() {
    let mut inputs = small_inputs();
    inputs.related = vec![related(
        5,
        0.8,
        "Cards are rebuilt when a candidate changes",
    )];
    let card = render(&inputs);
    assert!(
        card.text
            .contains("Related: Cards are rebuilt when a candidate changes [lesson:00000000-0000-0000-0000-000000000005]"),
        "{}",
        card.text
    );
    assert!(
        card.text.len() <= MAX_CARD_BYTES,
        "{} bytes",
        card.text.len()
    );
    let related_at = card.text.find("Related:");
    let footer_at = card.text.find("Full state:");
    assert!(related_at < footer_at, "{}", card.text);
    assert!(card.text.find("Unmet asks:") < related_at, "{}", card.text);
}

#[test]
fn render_without_related_items_matches_the_card_without_the_section() {
    let without = render(&small_inputs());
    assert!(!without.text.contains("Related"), "{}", without.text);
    let mut with_empty = small_inputs();
    with_empty.related = Vec::new();
    assert_eq!(render(&with_empty).text, without.text);
    assert_eq!(render(&with_empty).content_hash, without.content_hash);
}

#[test]
fn render_related_in_any_input_order_gives_the_same_text() {
    let items = vec![
        related(3, 0.5, "third by score"),
        related(1, 0.9, "tied first"),
        related(2, 0.9, "tied second"),
    ];
    let mut forward = small_inputs();
    forward.related = items.clone();
    let mut backward = small_inputs();
    backward.related = items.into_iter().rev().collect();
    assert_eq!(render(&forward).text, render(&backward).text);
    assert_eq!(
        render(&forward).content_hash,
        render(&backward).content_hash
    );
}

#[test]
fn render_related_cuts_a_long_lesson_line_to_eighty_bytes_before_fitting() {
    let mut inputs = small_inputs();
    inputs.state = None;
    inputs.related = vec![related(4, 0.7, &"lesson words ".repeat(30))];
    let card = render(&inputs);
    let line = card
        .text
        .lines()
        .find(|line| line.starts_with("Related:"))
        .unwrap_or_default();
    let body = line
        .trim_start_matches("Related: ")
        .split(" [lesson:")
        .next()
        .unwrap_or_default();
    assert!(body.len() <= 80, "{} bytes in {line:?}", body.len());
}

#[test]
fn render_with_full_sections_and_related_candidates_keeps_the_footer() {
    let card = render(&oversized_inputs("mike-tui-finish", "word "));
    assert!(card.text.contains("Full state:"), "{}", card.text);
    assert!(
        card.text.len() <= MAX_CARD_BYTES,
        "{} bytes",
        card.text.len()
    );
}
