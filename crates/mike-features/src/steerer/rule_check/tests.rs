use std::collections::BTreeMap;

use serde_json::json;

use super::compose::compose;
use super::questions::{key, questions};
use super::rules::{Rule, TurnTools, parse_rules, relevant};
use super::state::{TurnInput, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn rule(source: &str, text: &str, scope: &[&str]) -> Rule {
    Rule {
        source: source.to_owned(),
        text: text.to_owned(),
        scope: scope.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn answered(ps: &[f64]) -> Asked {
    Asked::answered(
        ps.iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    key(i),
                    Answer::Noul {
                        noul: *p,
                        confidence: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
}

#[test]
fn parse_rules_keeps_normative_bullets_and_the_path_scope() {
    let text = "---\npaths:\n  - \"crates/**/*.rs\"\n---\n# Rules\n- Write no Rust comments, never doc comments.\n- Short.\n- The weather is nice and this line is long enough.\n";
    let rules = parse_rules(".claude/rules/rust.md", text);
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].scope, vec!["crates/**/*.rs".to_owned()]);
    assert!(rules[0].text.contains("never doc comments"));
}

#[test]
fn parse_rules_reads_numbered_items() {
    let rules = parse_rules(
        "AGENTS.md",
        "1. Never pass --no-verify when pushing code.\n",
    );
    assert_eq!(rules.len(), 1);
}

#[test]
fn relevant_picks_rules_that_share_words_with_the_turn_and_is_bounded() {
    let rules = vec![
        rule(
            "AGENTS.md",
            "Never run raw cargo build; use scripts/cx for builds.",
            &[],
        ),
        rule(
            "AGENTS.md",
            "Never touch the capture database from tests.",
            &[],
        ),
        rule(
            "AGENTS.md",
            "Never publish a release without asking first today.",
            &[],
        ),
    ];
    let input = json!({"command": "cargo build --release"});
    let turn = TurnTools::from_calls([("Bash", &input)]);
    let picked = relevant(&rules, &turn, 1);
    assert_eq!(picked.len(), 1);
    assert!(picked[0].text.contains("raw cargo"));
}

#[test]
fn relevant_prefers_a_rule_whose_path_scope_matches_a_written_file() {
    let rules = vec![rule(
        ".claude/rules/rust.md",
        "Write no comments in Rust files, ever.",
        &["**/*.rs"],
    )];
    let input = json!({"file_path": "/repo/crates/a/src/lib.rs"});
    let turn = TurnTools::from_calls([("Edit", &input)]);
    assert_eq!(relevant(&rules, &turn, 5).len(), 1);
}

#[test]
fn relevant_returns_nothing_when_the_turn_shares_nothing() {
    let rules = vec![rule(
        "AGENTS.md",
        "Never deploy to production without asking.",
        &[],
    )];
    let input = json!({"file_path": "/repo/notes.txt"});
    let turn = TurnTools::from_calls([("Read", &input)]);
    assert!(relevant(&rules, &turn, 5).is_empty());
}

#[test]
fn compose_confident_violation_names_the_rule() {
    let outcome = compose(&answered(&[0.1, 0.9]), 2);
    assert_eq!(outcome.map(|o| (o.rule, o.violated)), Some((1, true)));
}

#[test]
fn compose_below_the_bar_is_not_a_violation() {
    let outcome = compose(&answered(&[0.1, 0.79]), 2);
    assert_eq!(outcome.map(|o| o.violated), Some(false));
}

#[test]
fn compose_failed_ask_is_none_so_no_note_is_left() {
    assert!(compose(&Asked::failed_with("timeout"), 2).is_none());
}

#[test]
fn questions_ask_one_noul_per_picked_rule_and_state_names_the_rules() {
    let a = rule("AGENTS.md", "Never do the first thing at all.", &[]);
    let b = rule("CLAUDE.md", "Always do the second thing here.", &[]);
    assert_eq!(questions(&[&a, &b]).len(), 2);
    let tools = TurnTools::default();
    let input = TurnInput {
        operator_ask: "x",
        turn_end: "y",
        tools: &tools,
    };
    let s = state(&input, &[&a, &b]);
    assert_eq!(s["rules"]["rule_1"], "Always do the second thing here.");
}
