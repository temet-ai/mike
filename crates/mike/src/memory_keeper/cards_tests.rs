use chrono::{DateTime, NaiveDate, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{MAX_CARD_BYTES, render};
use crate::store::persistence::work_threads::cards::{
    CardAsk, CardEntry, CardInputs, CardNote, CardPromise, CardState, CardThread,
};

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(seconds, 0).unwrap_or_default()
}

fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap_or_default()
}

fn thread() -> CardThread {
    CardThread {
        slug: "mike-tui-finish".to_owned(),
        title: Some("Finish the TUI".to_owned()),
        goal: Some("Ship the terminal UI".to_owned()),
        status: Some("implementing".to_owned()),
        branch: Some("tui-finish".to_owned()),
        worktree: Some(".claude/worktrees/tui-finish".to_owned()),
        last_session_uuid: Some(Uuid::from_u128(7)),
        updated_at: at(1_000),
    }
}

fn state(text: &str) -> CardState {
    CardState {
        tldr: "short".to_owned(),
        where_we_are: text.to_owned(),
        next_up: text.to_owned(),
        blockers: format!("{text}\n{text}\n{text}"),
        open_questions: format!("{text}\n{text}\n{text}"),
        updated_at: at(2_000),
    }
}

fn entry(id: i64, summary: &str) -> CardEntry {
    CardEntry {
        id,
        entry_date: day(7),
        session_uuid: Some(Uuid::from_u128(0x3f2a_9c1e_0000_0000_0000_0000_0000_0001)),
        summary: summary.to_owned(),
        created_at: at(3_000 + id),
    }
}

fn note(n: u128, title: &str) -> CardNote {
    CardNote {
        note_uuid: Uuid::from_u128(n),
        kind: "decision".to_owned(),
        title: title.to_owned(),
        created_at: at(4_000),
        updated_at: at(4_100),
    }
}

fn promise(id: i64, excerpt: &str) -> CardPromise {
    CardPromise {
        id,
        operator_turn_id: Some(id * 10),
        excerpt: excerpt.to_owned(),
        recorded_at: at(5_100),
    }
}

fn ask(turn: i64, unmet: &str) -> CardAsk {
    CardAsk {
        operator_turn_id: turn,
        unmet_ask: unmet.to_owned(),
        created_at: at(6_000),
    }
}

fn small_inputs() -> CardInputs {
    CardInputs {
        thread: thread(),
        state: Some(state("Render is done; the route is next")),
        handoff: Some(entry(12, "Handed off after the render landed")),
        decisions: vec![entry(14, "Keep the card under 1500 bytes")],
        notes: vec![note(9, "Cards are data, not instructions")],
        promises: vec![promise(7, "I will report back once the sweep has run")],
        unmet_asks: vec![ask(41, "Show the card in the thread page")],
    }
}

fn oversized_inputs(filler: &str) -> CardInputs {
    let long = filler.repeat(120);
    CardInputs {
        thread: thread(),
        state: Some(state(&long)),
        handoff: Some(entry(12, &long)),
        decisions: (20..30).map(|id| entry(id, &long)).collect(),
        notes: (1..6).map(|n| note(n, &long)).collect(),
        promises: (1..6).map(|id| promise(id, &long)).collect(),
        unmet_asks: (40..46).map(|turn| ask(turn, &long)).collect(),
    }
}

fn bracket_groups(text: &str) -> Vec<&str> {
    text.split('[')
        .skip(1)
        .filter_map(|rest| rest.split(']').next())
        .collect()
}

fn is_reference(token: &str) -> bool {
    ["thread:", "entry:", "note:", "promise:", "turn:"]
        .iter()
        .any(|prefix| {
            token
                .strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
        })
}

#[test]
fn render_equal_input_text_and_hash_identical() {
    let first = render(&small_inputs());
    let second = render(&small_inputs());
    assert_eq!(first, second);
    assert_eq!(first.text.as_bytes(), second.text.as_bytes());
}

#[test]
fn render_content_hash_is_sha256_of_text() {
    let rendered = render(&small_inputs());
    assert_eq!(
        rendered.content_hash,
        hex::encode(Sha256::digest(rendered.text.as_bytes()))
    );
}

#[test]
fn render_changed_input_changes_hash() {
    let mut changed = small_inputs();
    changed.decisions.push(entry(15, "Another decision"));
    assert_ne!(
        render(&small_inputs()).content_hash,
        render(&changed).content_hash
    );
}

#[test]
fn render_oversized_input_stays_within_budget() {
    for filler in ["all work and no play ", "tui ", "é🙂 "] {
        let rendered = render(&oversized_inputs(filler));
        assert!(
            rendered.text.len() <= MAX_CARD_BYTES,
            "{} bytes for filler {filler:?}",
            rendered.text.len()
        );
    }
}

#[test]
fn render_oversized_input_keeps_references_whole() {
    let rendered = render(&oversized_inputs("all work and no play "));
    let groups = bracket_groups(&rendered.text);
    assert!(!groups.is_empty());
    assert_eq!(
        rendered.text.matches('[').count(),
        rendered.text.matches(']').count()
    );
    for group in groups {
        assert!(
            group.split_whitespace().all(is_reference),
            "group {group:?} in {:?}",
            rendered.text
        );
    }
    for line in rendered.text.lines().filter(|l| !l.ends_with(':')) {
        assert!(line.ends_with(']'), "line without its reference: {line:?}");
    }
}

#[test]
fn render_oversized_input_drops_lowest_priority_sections_first() {
    let rendered = render(&oversized_inputs("all work and no play "));
    assert!(rendered.text.contains("Where we are:"), "{}", rendered.text);
    assert!(!rendered.text.contains("Unmet asks"), "{}", rendered.text);
    assert!(
        !rendered.text.contains("Open promises"),
        "{}",
        rendered.text
    );
}

#[test]
fn render_refs_match_the_references_in_the_text() {
    let rendered = render(&small_inputs());
    let listed: Vec<String> = rendered
        .refs
        .as_array()
        .map(|refs| {
            refs.iter()
                .filter_map(|r| r.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        listed.first().map(String::as_str),
        Some("thread:mike-tui-finish")
    );
    for expected in [
        "entry:12",
        "entry:14",
        "promise:7",
        "turn:70",
        "turn:41",
        "note:00000000-0000-0000-0000-000000000009",
    ] {
        assert!(
            listed.iter().any(|r| r == expected),
            "{expected} in {listed:?}"
        );
        assert!(rendered.text.contains(expected), "{expected} in text");
    }
    let mut unique = listed.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), listed.len());
}

#[test]
fn render_empty_sections_are_omitted() {
    let inputs = CardInputs {
        thread: thread(),
        state: Some(CardState {
            tldr: String::new(),
            where_we_are: "Halfway".to_owned(),
            next_up: "  ".to_owned(),
            blockers: String::new(),
            open_questions: "\n\n".to_owned(),
            updated_at: at(2_000),
        }),
        handoff: None,
        decisions: Vec::new(),
        notes: Vec::new(),
        promises: Vec::new(),
        unmet_asks: Vec::new(),
    };
    let rendered = render(&inputs);
    assert!(rendered.text.contains("Where we are: Halfway"));
    for absent in [
        "Next up",
        "Blockers",
        "Open questions",
        "Last handoff",
        "Decisions since",
        "Open promises",
        "Unmet asks",
    ] {
        assert!(
            !rendered.text.contains(absent),
            "{absent} in {}",
            rendered.text
        );
    }
}

#[test]
fn render_thread_without_state_renders_only_the_thread_line() {
    let inputs = CardInputs {
        thread: thread(),
        state: None,
        handoff: None,
        decisions: Vec::new(),
        notes: Vec::new(),
        promises: Vec::new(),
        unmet_asks: Vec::new(),
    };
    let rendered = render(&inputs);
    assert_eq!(
        rendered.text,
        "Work thread `mike-tui-finish` (implementing), branch `tui-finish`: Finish the TUI [thread:mike-tui-finish]"
    );
}

#[test]
fn render_sections_follow_the_fixed_order() {
    let rendered = render(&small_inputs());
    let positions: Vec<usize> = [
        "Work thread `mike-tui-finish` (implementing)",
        "Where we are:",
        "Next up:",
        "Blockers:",
        "Open questions:",
        "Last handoff:",
        "Decisions since:",
        "Open promises:",
        "Unmet asks:",
    ]
    .iter()
    .map(|heading| rendered.text.find(heading).unwrap_or(usize::MAX))
    .collect();
    assert!(
        positions.iter().all(|p| *p != usize::MAX),
        "{}",
        rendered.text
    );
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{positions:?} in {}",
        rendered.text
    );
}

#[test]
fn render_handoff_names_date_and_session() {
    let rendered = render(&small_inputs());
    assert!(
        rendered
            .text
            .contains("Last handoff: 2026-11-07 by session 3f2a9c1e: Handed off after the render landed [entry:12]"),
        "{}",
        rendered.text
    );
}

#[test]
fn render_frames_the_card_as_data() {
    let rendered = render(&small_inputs());
    assert!(
        rendered
            .text
            .starts_with("Work thread `mike-tui-finish` (implementing)")
    );
    assert!(rendered.text.lines().all(|line| !line.starts_with("You ")));
}

#[test]
fn render_card_json_lists_sections_in_order() {
    let rendered = render(&small_inputs());
    let keys: Vec<&str> = rendered.card["sections"]
        .as_array()
        .map(|sections| {
            sections
                .iter()
                .filter_map(|section| section["section"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        keys,
        [
            "where_we_are",
            "next_up",
            "blockers",
            "open_questions",
            "last_handoff",
            "decisions_since",
            "open_promises",
            "unmet_asks",
        ]
    );
    assert_eq!(rendered.card["thread"]["slug"], "mike-tui-finish");
}

#[test]
fn render_promise_keeps_the_end_of_the_turn_text() {
    let mut inputs = small_inputs();
    let tail = "I will report back once the sweep has run";
    inputs.promises = vec![promise(7, &format!("{} {tail}", "preamble ".repeat(60)))];
    let rendered = render(&inputs);
    assert!(rendered.text.contains(tail), "{}", rendered.text);
}
