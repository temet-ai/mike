use chrono::Utc;

use super::*;
use crate::steerer::anchor::Anchor;
use crate::steerer::fleet::judge::build_judge_user_message;
use crate::store::models::{Diagnostic, DiagnosticCategory};

fn entry(kind: &str, content: &str) -> AskLedgerEntryRow {
    AskLedgerEntryRow {
        id: 1,
        session_id: 1,
        operator_turn_id: None,
        kind: kind.to_string(),
        content: content.to_string(),
        created_at: Utc::now(),
    }
}

fn row(tool: &str, operation: &str, detail: Option<&str>, ok: bool) -> SessionActionRow {
    let is_bash = tool == "Bash";
    SessionActionRow {
        tool_name: tool.to_string(),
        file_path: if is_bash {
            None
        } else {
            detail.map(str::to_string)
        },
        command: if is_bash {
            detail.map(str::to_string)
        } else {
            None
        },
        has_result: true,
        result_is_error: Some(!ok),
        operation: Some(operation.to_string()),
    }
}

fn counts_of(rows: &[SessionActionRow]) -> SessionActionCounts {
    tally_counts(rows)
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn inputs(eval_text: &str, ask: &str) -> EvalInputs {
    EvalInputs {
        eval_text: eval_text.to_string(),
        operator_ask: ask.to_string(),
        last_user_message: ask.to_string(),
        trigger_point: "response",
        ..Default::default()
    }
}

#[test]
fn from_setting_maps_three_codes_and_defaults_unknown_to_v1() {
    assert_eq!(
        VerifierInputBudget::from_setting("uncapped"),
        VerifierInputBudget::Uncapped
    );
    assert_eq!(
        VerifierInputBudget::from_setting("verifier-input-v1"),
        VerifierInputBudget::V1
    );
    assert_eq!(
        VerifierInputBudget::from_setting("verifier-input-v1-wide"),
        VerifierInputBudget::V1Wide
    );
    assert_eq!(
        VerifierInputBudget::from_setting("half"),
        VerifierInputBudget::V1
    );
}

fn expected_message(f: &EvalInputs) -> String {
    let mut expected = Vec::new();
    if !f.operator_ask.is_empty() {
        expected.push(format!("User ask:\n{}", f.operator_ask));
    }
    if !f.last_user_message.is_empty() && f.last_user_message != f.operator_ask {
        expected.push(format!("Latest message:\n{}", f.last_user_message));
    }
    match f.turn_origin.as_str() {
        "subagent" => expected.push(format!(
            "Acting agent: subagent · {}",
            f.subagent_persona
                .as_deref()
                .unwrap_or("unknown specialist")
        )),
        "primary" => {
            expected.push("Acting agent: main orchestrator (not a specialist subagent)".to_string())
        }
        _ => {}
    }
    if !f.tool_calls_summary.is_empty() {
        expected.push(format!("Tool calls this turn:\n{}", f.tool_calls_summary));
    }
    let label = if f.trigger_point == "user_prompt" {
        "User prompt to evaluate:"
    } else {
        "Assistant response to evaluate:"
    };
    expected.push(format!("{label}\n{}", f.eval_text));
    if let Some(a) = &f.anchor {
        expected.push(format!(
            "Session context:\nState of play: {}\nNext: {}",
            a.state_of_play, a.next
        ));
    }
    if !f.diagnostics.is_empty() {
        let lines: Vec<String> = f
            .diagnostics
            .iter()
            .map(|d| {
                format!(
                    "[{}] {} — matched: {:?}",
                    d.category.as_str(),
                    d.advice,
                    d.matched_text
                )
            })
            .collect();
        expected.push(format!("Analysis findings:\n{}", lines.join("\n")));
    }
    expected.join("\n\n")
}

#[test]
fn uncapped_message_is_byte_identical_to_pre_change_builder() {
    let long_text = "the response ".repeat(2_000);
    let fixtures = [
        inputs("Done.", "fix it"),
        inputs(&long_text, &"ask ".repeat(900)),
        EvalInputs {
            tool_calls_summary: "1. Edit — a.rs\n2. Bash — cargo test\n".repeat(200),
            anchor: Some(Anchor {
                state_of_play: "x".repeat(2_000),
                next: "y".repeat(2_000),
            }),
            last_user_message: "different".to_string(),
            turn_origin: "primary".to_string(),
            ..inputs("Done.", "fix it")
        },
        EvalInputs {
            turn_origin: "subagent".to_string(),
            subagent_persona: Some("rust-developer".to_string()),
            trigger_point: "user_prompt",
            ..inputs("How?", "How?")
        },
        inputs(&"é".repeat(9_000), "ask"),
        EvalInputs {
            diagnostics: vec![Diagnostic {
                category: DiagnosticCategory::Overselling,
                analyzer: "overselling".to_string(),
                advice: "state what was verified".to_string(),
                matched_text: "perfectly \"done\"".to_string(),
                match_range: (0, 4),
                other_hits: 0,
                created_at: Utc::now(),
            }],
            ..inputs("Done.", "fix it")
        },
    ];
    for f in &fixtures {
        let b = VerifierInputBudget::Uncapped.apply(f);
        assert!(matches!(b.eval_text, Cow::Borrowed(_)));
        assert!(matches!(b.operator_ask, Cow::Borrowed(_)));
        assert!(matches!(b.tool_calls_summary, Cow::Borrowed(_)));
        let msg = build_judge_user_message(f, None, VerifierInputBudget::Uncapped);
        let expected = expected_message(f);
        assert_eq!(msg, expected);
    }
}

#[test]
fn uncapped_context_keeps_unabridged_headings_and_full_list() {
    let rows: Vec<SessionActionRow> = (0..300)
        .map(|i| row("Bash", "exec", Some(&format!("cmd {i}")), true))
        .collect();
    let raw = RawVerifierContext {
        ask_entries: vec![entry("initial", "a"), entry("clarification", "b")],
        trajectory_digest: String::new(),
        action_rows: (0..300)
            .map(|i| row("Bash", "exec", Some(&format!("cmd {i}")), true))
            .collect(),
        action_counts: None,
        thread_criteria: None,
        intake_criteria: None,
        checklist_criteria: Vec::new(),
    };
    let ctx = project_verifier_context(&raw, VerifierInputBudget::Uncapped);
    assert_eq!(ctx.session_action_list, build_session_action_list(&rows));
    assert_eq!(ctx.accumulated_ask, "[initial] a\n[clarification] b");
    let msg =
        build_judge_user_message(&inputs("x", "y"), Some(&ctx), VerifierInputBudget::Uncapped);
    assert!(msg.contains("every tool call this session has made"));
    assert!(msg.contains("full accreted intent"));
    assert!(msg.contains("300. Bash — cmd 299 → ok"));
}

#[test]
fn eval_text_over_cap_keeps_tail_with_earlier_marker() {
    let text = format!("{}{}", "a".repeat(10_000), "TAIL-END");
    let i = inputs(&text, "ask");
    let b = VerifierInputBudget::V1.apply(&i);
    assert!(b.eval_text.starts_with("[... "));
    assert!(b.eval_text.contains(" earlier characters not shown] "));
    assert!(b.eval_text.ends_with("TAIL-END"));
    assert!(chars(&b.eval_text) <= 4_000);
    let wide = VerifierInputBudget::V1Wide.apply(&i);
    assert!(chars(&wide.eval_text) <= 8_000 && chars(&wide.eval_text) > 4_000);
}

#[test]
fn eval_text_under_cap_is_unchanged() {
    let i = inputs("short answer", "ask");
    let b = VerifierInputBudget::V1.apply(&i);
    assert_eq!(b.eval_text, "short answer");
    assert!(matches!(b.eval_text, Cow::Borrowed(_)));
}

#[test]
fn operator_ask_over_cap_keeps_head_with_more_marker() {
    let ask = format!("HEAD{}", "z".repeat(5_000));
    let i = inputs("x", &ask);
    let b = VerifierInputBudget::V1.apply(&i);
    assert!(b.operator_ask.starts_with("HEAD"));
    assert!(b.operator_ask.ends_with(" more characters]"));
    assert!(b.operator_ask.contains(" [... "));
    assert!(chars(&b.operator_ask) <= 1_500);
}

#[test]
fn accumulated_ask_keeps_first_and_newest_and_counts_between() {
    let entries: Vec<AskLedgerEntryRow> = (0..30)
        .map(|i| {
            entry(
                if i == 0 { "initial" } else { "clarification" },
                &format!("ask-{i:02} {}", "w".repeat(600)),
            )
        })
        .collect();
    let out = project_accumulated_ask(&entries, &V1_CAPS);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines[0].starts_with("[initial] ask-00"));
    assert!(lines[1].contains("asks between the first and the newest are not shown)"));
    assert!(lines.last().is_some_and(|l| l.contains("ask-29")));
    assert!(chars(&out) <= 5_000);
    let between: usize = lines[1]
        .trim_start_matches('(')
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    assert_eq!(between + (lines.len() - 2) + 1, 30);
}

#[test]
fn accumulated_ask_under_cap_is_whole() {
    let entries = vec![entry("initial", "a"), entry("clarification", "b")];
    assert_eq!(
        project_accumulated_ask(&entries, &V1_CAPS),
        "[initial] a\n[clarification] b"
    );
}

#[test]
fn accumulated_ask_entry_over_cap_is_head_cut() {
    let entries = vec![entry("initial", &"q".repeat(3_000))];
    let out = project_accumulated_ask(&entries, &V1_CAPS);
    assert!(out.starts_with("[initial] qqq"));
    assert!(out.contains(" more characters]"));
    assert!(chars(&out) <= 1_200 + "[initial] ".len());
}

#[test]
fn criteria_cut_at_whole_criterion() {
    let criteria: String = (0..40)
        .map(|i| {
            format!(
                "- [operator asked] criterion number {i} {}\n  (their words: \"{}\")",
                "c".repeat(100),
                "w".repeat(60)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let out = cut_entries_head(&criteria, V1_CAPS.criteria, starts_criterion).into_owned();
    assert!(chars(&out) <= 4_500);
    let body: Vec<&str> = out.lines().collect();
    let last_marker = body.last().copied().unwrap_or_default();
    assert!(last_marker.ends_with(" further lines not shown)"));
    let kept = &body[..body.len() - 1];
    assert!(
        kept.last()
            .is_some_and(|l| l.starts_with("  (their words:")),
        "cut lands between criteria: {kept:?}"
    );
    assert_eq!(kept.len() % 2, 0);
}

#[test]
fn prior_fires_keep_newest() {
    let digest: String = (1..=10)
        .map(|i| format!("{i}. 2026-09-24 00:00:00Z — {}", "r".repeat(120)))
        .collect::<Vec<_>>()
        .join("\n");
    let out = project_prior_fires(digest, VerifierInputBudget::V1);
    assert!(out.starts_with("1. "));
    assert!(out.contains("\n2. "));
    assert!(!out.contains("\n10. "));
    assert!(out.ends_with(" further lines not shown)"));
    assert!(chars(&out) <= 800);
    assert_eq!(
        project_prior_fires("1. x".to_string(), VerifierInputBudget::Uncapped),
        "1. x"
    );
}

#[test]
fn every_section_within_its_cap_under_v1_and_wide() {
    let big = "b".repeat(50_000);
    let rows = mixed_rows(900);
    for (budget, caps) in [
        (VerifierInputBudget::V1, &V1_CAPS),
        (VerifierInputBudget::V1Wide, &V1_WIDE_CAPS),
    ] {
        let i = EvalInputs {
            tool_calls_summary: "1. Edit — a.rs\n".repeat(400),
            anchor: Some(Anchor {
                state_of_play: big.clone(),
                next: big.clone(),
            }),
            last_user_message: big.clone(),
            ..inputs(&big, &big)
        };
        let b = budget.apply(&i);
        assert!(chars(&b.operator_ask) <= caps.operator_ask);
        assert!(chars(&b.last_user_message) <= caps.last_user_message);
        assert!(chars(&b.tool_calls_summary) <= caps.tool_calls_summary);
        assert!(chars(&b.eval_text) <= caps.eval_text);
        let (sop, next) = b
            .anchor
            .as_ref()
            .map(|(a, n)| (chars(a), chars(n)))
            .unwrap_or_default();
        assert!(sop <= caps.anchor_field && next <= caps.anchor_field);
        let entries: Vec<AskLedgerEntryRow> =
            (0..30).map(|_| entry("clarification", &big)).collect();
        assert!(chars(&project_accumulated_ask(&entries, caps)) <= caps.accumulated_ask);
        let loaded = &rows[rows.len() - caps.action_rows_loaded as usize..];
        let list = project_action_list(loaded, Some(counts_of(&rows)), caps);
        assert!(chars(&list) <= caps.session_actions);
        let long_criteria = "- criterion\n".repeat(2_000);
        assert!(
            chars(&cut_entries_head(
                &long_criteria,
                caps.criteria,
                starts_criterion
            )) <= caps.criteria
        );
    }
}

#[test]
fn capped_headings_replace_the_unabridged_wording() {
    let raw = RawVerifierContext {
        ask_entries: vec![entry("initial", "a")],
        trajectory_digest: String::new(),
        action_rows: vec![row("Bash", "exec", Some("ls"), true)],
        action_counts: None,
        thread_criteria: None,
        intake_criteria: None,
        checklist_criteria: Vec::new(),
    };
    let ctx = project_verifier_context(&raw, VerifierInputBudget::V1);
    assert_eq!(ctx.budget, VerifierInputBudget::V1);
    let msg = build_judge_user_message(&inputs("x", "y"), Some(&ctx), VerifierInputBudget::V1);
    assert!(msg.contains(CAPPED_ASK_HEADING));
    assert!(msg.contains(CAPPED_ACTION_LIST_HEADING));
    assert!(!msg.contains("unabridged"));
    assert!(!msg.contains("full accreted intent"));
}

#[test]
fn markers_contain_no_fidelity_banned_token() {
    let banned: Vec<String> = [
        "\u{2026}",
        "[trunc|ated]",
        "trun|cate",
        "line-|clamp",
        "text-|ellipsis",
        "_pre|view",
        "_sum|mary",
        "cl|ip(",
        "head|_n(",
    ]
    .iter()
    .map(|t| t.replace('|', ""))
    .collect();
    let rows = mixed_rows(300);
    let projected = project_action_list(&rows[100..], Some(counts_of(&rows)), &V1_CAPS);
    let samples = [
        more_marker(12_345),
        earlier_marker(12_345),
        format!("\n({} further lines not shown)", 7),
        CAPPED_ASK_HEADING.to_string(),
        CAPPED_ACTION_LIST_HEADING.to_string(),
        projected,
        cut_head(&"x".repeat(9_999), 100).into_owned(),
        cut_tail(&"x".repeat(9_999), 100).into_owned(),
    ];
    for s in &samples {
        for token in &banned {
            assert!(
                !s.contains(token),
                "marker text carries banned token {token}: {s}"
            );
        }
    }
}

#[test]
fn counts_are_unicode_scalars() {
    let text = "é".repeat(5_000);
    let out = cut_head(&text, 100);
    assert!(chars(&out) <= 100);
    assert!(out.starts_with("éé"));
    let tail = cut_tail(&text, 100);
    assert!(chars(&tail) <= 100);
    assert!(tail.ends_with("éé"));
    assert!(out.contains("more characters]"));
}

#[test]
fn cut_marker_reports_exact_omitted_count() {
    let text = "k".repeat(1_000);
    let out = cut_head(&text, 200).into_owned();
    let kept = out.find(" [... ").unwrap_or(0);
    let reported: usize = out[kept..]
        .trim_start_matches(" [... ")
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    assert_eq!(kept + reported, 1_000);
}

#[test]
fn for_steerer_kind_gives_non_verifier_kinds_uncapped() {
    let settings: SharedSettingsOverlay = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
        crate::settings::SettingsOverlay::default(),
    ));
    assert_eq!(
        VerifierInputBudget::for_steerer_kind(&settings, "verifier"),
        VerifierInputBudget::V1
    );
    assert_eq!(
        VerifierInputBudget::for_steerer_kind(&settings, "refocus"),
        VerifierInputBudget::Uncapped
    );
}

fn mixed_rows(n: usize) -> Vec<SessionActionRow> {
    (0..n)
        .map(|i| match i % 4 {
            0 => row("Edit", "write", Some(&format!("/src/f{i}.rs")), true),
            1 => row("Bash", "exec", Some(&format!("cargo test {i}")), i % 8 != 1),
            2 => row("Agent", "dispatch", None, true),
            _ => row("Bash", "exec", Some("ls"), true),
        })
        .collect()
}
