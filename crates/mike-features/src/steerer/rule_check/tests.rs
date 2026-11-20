use std::collections::BTreeMap;

use serde_json::json;

use super::compose::compose;
use super::questions::{applies_key, key, questions, requires_key};
use super::rules::{Rule, TurnTools, cut_at_whitespace, parse_rules, relevant};
use super::state::{TurnInput, state};
use crate::llm::decisions::point::Asked;
use crate::llm::decisions::{Answer, Question};

fn rule(source: &str, text: &str, scope: &[&str]) -> Rule {
    Rule {
        source: source.to_owned(),
        text: text.to_owned(),
        scope: scope.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn noul(p: f64) -> Answer {
    Answer::Noul {
        noul: p,
        confidence: None,
    }
}

fn answered_with(ps: &[f64], applies: f64) -> Asked {
    Asked::answered(
        ps.iter()
            .enumerate()
            .flat_map(|(i, p)| [(key(i), noul(*p)), (applies_key(i), noul(applies))])
            .collect::<BTreeMap<_, _>>(),
    )
}

fn answered(ps: &[f64]) -> Asked {
    answered_with(ps, 0.95)
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
    assert_eq!(
        outcome.map(|o| (o.rule, o.violated, o.held)),
        Some((1, true, false))
    );
}

#[test]
fn compose_below_the_warn_bar_is_not_a_violation() {
    let outcome = compose(&answered(&[0.1, 0.89]), 2);
    assert_eq!(outcome.map(|o| (o.violated, o.held)), Some((false, false)));
}

#[test]
fn compose_band_warns_at_the_top_holds_at_the_bottom_and_neither_between() {
    let band = |p: f64| compose(&answered(&[p]), 1).map(|o| (o.violated, o.held));
    assert_eq!(band(0.9), Some((true, false)));
    assert_eq!(band(0.5), Some((false, false)));
    assert_eq!(band(0.31), Some((false, false)));
    assert_eq!(band(0.3), Some((false, true)));
    assert_eq!(band(0.05), Some((false, true)));
}

#[test]
fn compose_takes_the_likelier_of_the_forbids_and_requires_answers() {
    let asked = Asked::answered(BTreeMap::from([
        (key(0), noul(0.1)),
        (applies_key(0), noul(0.9)),
        (requires_key(0), noul(0.95)),
    ]));
    let outcome = compose(&asked, 1);
    assert_eq!(
        outcome.map(|o| (o.probability, o.violated)),
        Some((0.95, true))
    );
}

#[test]
fn compose_rule_about_work_the_turn_did_not_do_is_no_violation() {
    let outcome = compose(&answered_with(&[0.97], 0.4), 1);
    assert_eq!(outcome.map(|o| (o.violated, o.held)), Some((false, true)));
}

#[test]
fn questions_ask_a_commit_obligation_only_when_the_turn_committed() {
    let a = rule(
        "AGENTS.md",
        "Before you commit, gitleaks must report no leaks.",
        &[],
    );
    assert!(!questions(&[&a], false).contains_key("rule_0_requires"));
    assert!(questions(&[&a], true).contains_key("rule_0_requires"));
}

#[test]
fn turn_tools_count_written_paths_and_commits_only() {
    let read = json!({"file_path": "/repo/a.rs"});
    let edit = json!({"file_path": "/elsewhere/b.rs"});
    let commit = json!({"command": "cd /repo && git commit -m x"});
    let tools = TurnTools::from_calls([("Read", &read), ("Edit", &edit), ("Bash", &commit)]);
    assert_eq!(tools.paths, vec!["/elsewhere/b.rs".to_owned()]);
    assert!(tools.committed);
    assert!(tools.writes_only_outside("/repo"));
    assert!(!tools.writes_only_outside("/elsewhere"));
    assert!(!TurnTools::default().writes_only_outside("/repo"));
}

#[test]
fn compose_failed_ask_is_none_so_no_note_is_left() {
    assert!(compose(&Asked::failed_with("timeout"), 2).is_none());
}

#[test]
fn is_obligation_needs_must_or_always_as_a_whole_word() {
    let yes = rule(
        "AGENTS.md",
        "Tests must pass before the push goes out.",
        &[],
    );
    let also = rule(
        "AGENTS.md",
        "Always run the formatter before a commit.",
        &[],
    );
    let no = rule(
        "AGENTS.md",
        "Never pass --no-verify when pushing code.",
        &[],
    );
    let part = rule(
        "AGENTS.md",
        "Mustard and alwayson are not triggers, never.",
        &[],
    );
    assert!(yes.is_obligation());
    assert!(also.is_obligation());
    assert!(!no.is_obligation());
    assert!(!part.is_obligation());
}

fn instructions_of(asked: &BTreeMap<String, Question>, name: &str) -> String {
    match asked.get(name) {
        Some(Question::Noul { instructions, .. }) => instructions.clone(),
        _ => String::new(),
    }
}

#[test]
fn questions_ask_only_the_forbids_side_for_a_rule_that_is_not_an_obligation() {
    let a = rule("AGENTS.md", "Never do the first thing at all.", &[]);
    let asked = questions(&[&a], false);
    assert_eq!(asked.len(), 2);
    assert!(asked.contains_key("rule_0_applies"));
    assert_eq!(
        instructions_of(&asked, "rule_0"),
        "Does a call in `tool_calls` or a path in `files_written` do what `rules.rule_0` forbids?"
    );
}

#[test]
fn questions_add_the_leaves_out_question_for_an_obligation_only() {
    let a = rule("AGENTS.md", "Never do the first thing at all.", &[]);
    let b = rule("CLAUDE.md", "Always do the second thing here.", &[]);
    let asked = questions(&[&a, &b], false);
    assert_eq!(asked.len(), 5);
    assert!(asked.contains_key("rule_1_requires"));
    assert!(!asked.contains_key("rule_0_requires"));
    assert_eq!(
        instructions_of(&asked, "rule_1_requires"),
        "Do `tool_calls` and `files_written` leave out what `rules.rule_1` requires?"
    );
}

#[test]
fn state_names_the_rules_and_leaves_out_the_operator_ask_and_the_turn_end() {
    let a = rule("AGENTS.md", "Never do the first thing at all.", &[]);
    let b = rule("CLAUDE.md", "Always do the second thing here.", &[]);
    let tools = TurnTools::default();
    let input = TurnInput { tools: &tools };
    let s = state(&input, &[&a, &b]);
    assert_eq!(s["rules"]["rule_1"], "Always do the second thing here.");
    assert!(s.get("operator_ask").is_none());
    assert!(s.get("turn_end").is_none());
}

#[test]
fn state_numbers_tool_calls_and_keeps_a_matching_command_whole() {
    let long = format!("cargo build {} --release", "x ".repeat(200));
    let first = json!({"command": "ls -la"});
    let second = json!({"command": long});
    let tools = TurnTools::from_calls([("Bash", &first), ("Bash", &second)]);
    let a = rule(
        "AGENTS.md",
        "Never run raw cargo build in the checkout.",
        &[],
    );
    let s = state(&TurnInput { tools: &tools }, &[&a]);
    let lines = s["tool_calls"].as_array().cloned().unwrap_or_default();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "T01| Bash: ls -la");
    let whole = lines[1].as_str().unwrap_or_default();
    assert!(whole.starts_with("T02| Bash: cargo build"));
    assert!(whole.ends_with("--release"));
    assert!(whole.chars().count() > 400);
}

#[test]
fn state_cuts_a_command_no_rule_names_at_whitespace() {
    let long = format!("lsof {} -i", "word ".repeat(80));
    let input = json!({"command": long});
    let tools = TurnTools::from_calls([("Bash", &input)]);
    let a = rule(
        "AGENTS.md",
        "Never run raw cargo build in the checkout.",
        &[],
    );
    let s = state(&TurnInput { tools: &tools }, &[&a]);
    let line = s["tool_calls"][0].as_str().unwrap_or_default();
    assert!(line.starts_with("T01| Bash: lsof word"));
    assert!(line.chars().count() < 215);
}
