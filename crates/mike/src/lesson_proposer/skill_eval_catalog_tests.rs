use serde_json::json;

use super::catalog::{SAMPLE_SIZE, gather, parse_listing};
use crate::store::persistence::lessons::skill_draft_eval::ListingBody;

fn body(system: &str, messages: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "model": "m",
        "messages": messages,
        "system": [{"type": "text", "text": system}]
    }))
    .unwrap_or_default()
}

fn listing(entries: &[(&str, &str)]) -> String {
    let mut out = String::from(
        "Intro.\n\nThe following skills are available for use with the Skill tool:\n\n",
    );
    for (name, description) in entries {
        out.push_str(&format!("- {name}: {description}\n"));
    }
    out.push_str("\nSomething after the list.");
    out
}

fn carrying(id: i64, entries: &[(&str, &str)]) -> ListingBody {
    ListingBody {
        request_id: id,
        body: body(&listing(entries), json!([])),
    }
}

#[test]
fn parse_listing_reads_every_entry_of_the_system_prompt_list() {
    let skills = parse_listing(&body(
        &listing(&[
            ("delegate", "Orchestrate a build."),
            (
                "anthropic-skills:docx",
                "Use for Word files: tracked changes.",
            ),
        ]),
        json!([]),
    ));
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0].folder, "delegate");
    assert_eq!(skills[0].description, "Orchestrate a build.");
    assert_eq!(skills[1].folder, "anthropic-skills:docx");
    assert_eq!(
        skills[1].description,
        "Use for Word files: tracked changes."
    );
}

#[test]
fn parse_listing_finds_a_list_sent_in_a_user_message() {
    let messages = json!([{
        "role": "user",
        "content": [{"type": "text", "text": format!("<system-reminder>\n{}</system-reminder>", listing(&[("loop", "Run on an interval.")]))}]
    }]);
    let skills = parse_listing(&body("No list here.", messages));
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].folder, "loop");
}

#[test]
fn parse_listing_joins_a_wrapped_description_and_stops_at_the_blank_line() {
    let text = "The following skills are available for use with the Skill tool:\n\n- a: first line\n  second line\n- b: other\n\n- c: after the blank line\n";
    let skills = parse_listing(&body(text, json!([])));
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0].description, "first line second line");
    assert_eq!(skills[1].folder, "b");
}

#[test]
fn parse_listing_returns_nothing_for_a_body_without_a_list_or_without_json() {
    assert!(parse_listing(&body("plain system prompt", json!([]))).is_empty());
    assert!(parse_listing(b"not json").is_empty());
    assert!(
        parse_listing(&body(
            "skills are available for use with the Skill tool:\n\nnot an entry",
            json!([])
        ))
        .is_empty()
    );
}

#[test]
fn gather_unions_the_lists_skips_the_drafts_own_name_and_records_the_requests() {
    let bodies = vec![
        carrying(11, &[("a", "one"), ("mine", "old version"), ("b", "two")]),
        carrying(12, &[("b", "two again"), ("c", "three")]),
        ListingBody {
            request_id: 13,
            body: body("nothing", json!([])),
        },
    ];
    let real = gather(&bodies, "mine", 7);
    let names: Vec<&str> = real.skills.iter().map(|s| s.folder.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "c"]);
    assert_eq!(real.seen, 3);
    assert_eq!(real.request_ids, vec![11, 12]);
    assert_eq!(real.skills[1].description, "two");
}

#[test]
fn gather_samples_a_long_list_to_a_fixed_size_the_same_way_every_time() {
    let names: Vec<String> = (0..SAMPLE_SIZE + 40)
        .map(|i| format!("skill-{i}"))
        .collect();
    let entries: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "d")).collect();
    let bodies = vec![carrying(1, &entries)];
    let first = gather(&bodies, "mine", 5);
    assert_eq!(first.seen, SAMPLE_SIZE + 40);
    assert_eq!(first.skills.len(), SAMPLE_SIZE);
    assert_eq!(first, gather(&bodies, "mine", 5));
    assert_ne!(first.skills, gather(&bodies, "mine", 6).skills);
}

#[test]
fn gather_keeps_a_short_list_whole() {
    let real = gather(&[carrying(1, &[("a", "x"), ("b", "y")])], "mine", 1);
    assert_eq!(real.skills.len(), 2);
    assert!(gather(&[], "mine", 1).skills.is_empty());
}
