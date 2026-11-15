use std::collections::BTreeMap;

use serde_json::json;

use super::compose::compose;
use super::questions::{key, questions};
use super::rules::{Rule, TurnTools, cut_at_whitespace, parse_rules, relevant};
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
fn parse_rules_joins_a_bullet_wrapped_over_several_lines() {
    let text = "- Use `TestDb` from `test_support/`; never the capture DB\n  or its hosting instance.\n- Next rule must stand alone here.\n";
    let rules = parse_rules("rules.md", text);
    assert_eq!(rules.len(), 2);
    assert!(
        rules[0]
            .text
            .ends_with("never the capture DB or its hosting instance.")
    );
    assert_eq!(rules[1].text, "Next rule must stand alone here.");
}

#[test]
fn relevant_ignores_filler_words_shared_with_the_turn() {
    let rules = vec![rule(
        "AGENTS.md",
        "Hand-edit the export only for approved token changes.",
        &[],
    )];
    let input = json!({"command": "rg -n the and for use src"});
    let turn = TurnTools::from_calls([("Bash", &input)]);
    assert!(relevant(&rules, &turn, 5).is_empty());
}

#[test]
fn parse_rules_does_not_treat_only_as_a_trigger() {
    let text = "- Read-only SQL: `PGOPTIONS='-c statement_timeout=5000'` docker exec armitage-pg psql.\n- Use it for the one thing only when asked politely.\n";
    assert!(parse_rules("AGENTS.md", text).is_empty());
}

#[test]
fn parse_rules_matches_trigger_words_whole_not_as_substrings() {
    let text = "- Nevertheless the build takes a while to finish here.\n- Mustard goes on the sandwich at lunchtime today.\n- Hooks refuses nothing but a hook refuses a bad push.\n- Alwaysonline is a product name with many words.\n";
    let rules = parse_rules("AGENTS.md", text);
    assert_eq!(rules.len(), 1);
    assert!(rules[0].text.starts_with("Hooks refuses"));
}

#[test]
fn parse_rules_keeps_inflected_and_apostrophe_triggers() {
    let text = "- The hook forbids raw cargo commands in this checkout.\n- You don't push without the hook running first.\n- The linter rejects comments in Rust source files.\n";
    assert_eq!(parse_rules("AGENTS.md", text).len(), 3);
}

#[test]
fn parse_rules_keeps_every_continuation_line_of_a_rule_together() {
    let text = "- Never run raw cargo; go through the wrapper that\n  loads the env and takes the build lock\nand picks the target directory.\n\n- Next must stand alone in this file.\n";
    let rules = parse_rules("rules.md", text);
    assert_eq!(rules.len(), 2);
    assert!(rules[0].text.ends_with("and picks the target directory."));
    assert!(rules[0].text.contains("takes the build lock and picks"));
}

#[test]
fn tool_lines_are_cut_at_whitespace_not_mid_word() {
    let command = format!("{} PGOPTIONS=-c statement_timeout=5000", "word ".repeat(39));
    let input = json!({ "command": command });
    let turn = TurnTools::from_calls([("Bash", &input)]);
    let line = &turn.lines[0];
    assert!(line.starts_with("Bash: word word"));
    assert!(line.ends_with("word"));
    assert!(line.chars().count() <= 206);
}

#[test]
fn cut_at_whitespace_keeps_a_short_text_and_a_cut_that_lands_on_a_space() {
    assert_eq!(cut_at_whitespace("short text", 50), "short text");
    assert_eq!(cut_at_whitespace("alpha beta gamma", 10), "alpha beta");
    assert_eq!(cut_at_whitespace("alpha beta gamma", 8), "alpha");
    assert_eq!(cut_at_whitespace("longtokenwithoutspaces", 8), "longtoke");
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
