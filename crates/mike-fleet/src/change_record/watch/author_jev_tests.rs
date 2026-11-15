use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::json;

use super::*;
use crate::launch::fleet::decisions::test_support::{
    DecisionsFixture, choice_body, jev_decider, stub_jev,
};
use crate::llm::decisions::Answer;
use crate::llm::decisions::record::DecisionMode;

fn edit(session: i64, text: &str) -> EditCall {
    EditCall {
        session_id: session,
        tool_use_id: session * 10,
        tool_name: "Write".to_owned(),
        at: Utc::now(),
        ran_from: Utc::now(),
        ran_to: None,
        failed: Some(false),
        input: Some(json!({"content": text})),
    }
}

fn run(session: i64) -> ShellRun {
    ShellRun {
        session_id: session,
        started: Utc::now(),
        finished: None,
        names_checkout: false,
        command: None,
    }
}

fn now_window() -> Window {
    Window {
        from: Utc::now(),
        to: Utc::now(),
    }
}

fn picked_with(key: &str, p: f64, writes: f64) -> Asked {
    Asked::answered(BTreeMap::from([
        (
            WHO.to_owned(),
            Answer::Choice {
                choice: key.to_owned(),
                probabilities: BTreeMap::from([(key.to_owned(), p)]),
                confidence: None,
            },
        ),
        (
            WRITES.to_owned(),
            Answer::Noul {
                noul: writes,
                confidence: None,
            },
        ),
    ]))
}

fn picked(key: &str, p: f64) -> Asked {
    picked_with(key, p, 0.9)
}

fn body(writes: f64) -> serde_json::Value {
    let mut value = choice_body(WHO, "s7", &["s3", "unknown"]);
    value["answers"][WRITES] = json!({"type": "noul", "noul": writes});
    value
}

#[test]
fn compose_a_candidate_key_gives_that_session() {
    assert_eq!(compose(&picked("s7", 0.9), &[3, 7]), Some((7, 0.9)));
}

#[test]
fn compose_unknown_gives_no_author() {
    assert_eq!(compose(&picked("unknown", 0.9), &[3, 7]), None);
}

#[test]
fn compose_a_pick_below_the_floor_gives_no_author() {
    assert_eq!(compose(&picked("s7", 0.59), &[3, 7]), None);
    assert_eq!(compose(&picked("s7", 0.6), &[3, 7]), Some((7, 0.6)));
}

#[test]
fn compose_without_a_writing_call_gives_no_author() {
    assert_eq!(compose(&picked_with("s7", 0.95, 0.69), &[3, 7]), None);
    assert_eq!(
        compose(&picked_with("s7", 0.95, 0.7), &[3, 7]),
        Some((7, 0.95))
    );
}

#[test]
fn questions_ask_the_choice_and_the_presence_noul_in_one_request() {
    let asked = questions(&[3, 7]);
    assert_eq!(asked.len(), 2);
    let Some(Question::Choice { criteria, .. }) = asked.get(WHO) else {
        panic!("who is not a choice");
    };
    assert_eq!(
        criteria.get("s3").map(String::as_str),
        Some(
            "Session 3: a command or edit tool call that overlaps the change window writes `changed_file`."
        )
    );
    assert!(criteria.contains_key(UNKNOWN));
    let Some(Question::Noul { instructions, .. }) = asked.get(WRITES) else {
        panic!("writes is not a noul");
    };
    assert_eq!(
        instructions,
        "Does any candidate's command or edit that overlaps the change window write `changed_file`?"
    );
}

#[test]
fn compose_a_failed_call_gives_no_author() {
    assert_eq!(compose(&Asked::failed_with("timeout"), &[3, 7]), None);
}

#[test]
fn state_reports_each_candidates_edits_and_whether_the_text_is_in_the_file() {
    let calls = vec![edit(3, "alpha"), edit(7, "beta")];
    let runs = vec![run(7)];
    let facts = Facts {
        path: "src/a.rs",
        calls: &calls,
        runs: &runs,
        after: Some("alpha only"),
        changed: now_window(),
    };
    let state = state(&[3, 7], &facts);
    let first = &state["candidate_sessions"][0];
    let second = &state["candidate_sessions"][1];
    assert_eq!(first["id"], "s3");
    assert_eq!(
        first["edit_tool_calls_naming_the_file"][0]["its_text_is_in_the_final_file"],
        true
    );
    assert_eq!(
        second["edit_tool_calls_naming_the_file"][0]["its_text_is_in_the_final_file"],
        false
    );
    assert_eq!(
        second["shell_commands_running_in_the_checkout"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn state_with_shell_runs_shows_each_command_and_the_change_window() {
    let runs = vec![ShellRun {
        command: Some("git worktree remove .claude/worktrees/x".to_owned()),
        ..run(7)
    }];
    let facts = Facts {
        path: "/work/alpha",
        calls: &[],
        runs: &runs,
        after: None,
        changed: now_window(),
    };
    let state = state(&[3, 7], &facts);
    assert_eq!(
        state["candidate_sessions"][1]["shell_commands_running_in_the_checkout"][0]["command"],
        "git worktree remove .claude/worktrees/x"
    );
    assert!(state.get("changed_between").is_none());
}

#[tokio::test]
async fn resolve_on_an_answered_choice_names_the_session() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, body(0.9)).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let facts = Facts {
        path: "a.rs",
        calls: &[],
        runs: &[],
        after: None,
        changed: now_window(),
    };
    let who = resolve(&decider, &[3, 7], &facts).await;
    assert_eq!(
        who,
        Some(Authorship::Judged {
            session: 7,
            percent: 80
        })
    );
    fx.finish().await
}

#[tokio::test]
async fn resolve_when_no_overlapping_call_writes_the_file_leaves_the_author_unknown()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, body(0.2)).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let facts = Facts {
        path: "a.rs",
        calls: &[],
        runs: &[],
        after: None,
        changed: now_window(),
    };
    assert_eq!(resolve(&decider, &[3, 7], &facts).await, None);
    fx.finish().await
}

#[tokio::test]
async fn resolve_with_a_failing_client_leaves_the_author_unknown() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(500, json!({})).await;
    let decider = jev_decider(&server, &fx.handle, POINT, DecisionMode::On)?;
    let facts = Facts {
        path: "a.rs",
        calls: &[],
        runs: &[],
        after: None,
        changed: now_window(),
    };
    assert_eq!(resolve(&decider, &[3, 7], &facts).await, None);
    fx.finish().await
}

#[test]
fn state_computes_the_time_order_in_code_and_sends_no_raw_timestamps() {
    let now = Utc::now();
    let changed = Window {
        from: now - chrono::Duration::seconds(10),
        to: now,
    };
    let inside = EditCall {
        ran_from: now - chrono::Duration::seconds(5),
        ran_to: Some(now - chrono::Duration::seconds(3)),
        ..edit(3, "alpha")
    };
    let before = EditCall {
        ran_from: now - chrono::Duration::seconds(60),
        ran_to: Some(now - chrono::Duration::seconds(40)),
        ..edit(3, "beta")
    };
    let runs = vec![ShellRun {
        started: now - chrono::Duration::seconds(2),
        finished: Some(now + chrono::Duration::seconds(8)),
        ..run(7)
    }];
    let calls = vec![inside, before];
    let facts = Facts {
        path: "src/a.rs",
        calls: &calls,
        runs: &runs,
        after: None,
        changed,
    };
    let state = state(&[3, 7], &facts);
    let edits = &state["candidate_sessions"][0]["edit_tool_calls_naming_the_file"];
    assert_eq!(edits[0]["overlaps_change_window"], "yes");
    assert_eq!(edits[0]["ended_seconds_after_change"], -3);
    assert_eq!(edits[1]["overlaps_change_window"], "no");
    assert_eq!(edits[1]["ended_seconds_after_change"], -40);
    let shell = &state["candidate_sessions"][1]["shell_commands_running_in_the_checkout"][0];
    assert_eq!(shell["overlaps_change_window"], "yes");
    assert_eq!(shell["ended_seconds_after_change"], 8);
    for raw in ["at", "ran_from", "ran_to"] {
        assert!(edits[0].get(raw).is_none());
    }
    for raw in ["started", "finished"] {
        assert!(shell.get(raw).is_none());
    }
}

#[test]
fn trim_command_keeps_a_short_command_whole() {
    assert_eq!(
        trim_command("git add src/a.rs", "src/a.rs"),
        "git add src/a.rs"
    );
}

#[test]
fn trim_command_long_command_keeps_the_lines_naming_the_path() {
    let noise = "echo unrelated\n".repeat(60);
    let command = format!("{noise}sed -i s/a/b/ src/a.rs\n{noise}");
    let trimmed = trim_command(&command, "src/a.rs");
    assert_eq!(trimmed, "sed -i s/a/b/ src/a.rs");
}

#[test]
fn trim_command_long_line_is_cut_to_300_chars_around_the_path() {
    let command = format!("{}src/a.rs{}", "x ".repeat(400), " y".repeat(400));
    let trimmed = trim_command(&command, "src/a.rs");
    assert_eq!(trimmed.chars().count(), 300);
    assert!(trimmed.contains("src/a.rs"));
}

#[test]
fn trim_command_long_command_without_the_path_keeps_its_head() {
    let command = "echo unrelated ".repeat(60);
    let trimmed = trim_command(&command, "src/a.rs");
    assert_eq!(trimmed.chars().count(), 300);
    assert!(command.starts_with(&trimmed));
}

#[test]
fn span_covers_an_edit_window_that_starts_before_the_shell_window() {
    let now = Utc::now();
    let edit = Window {
        from: now - chrono::Duration::seconds(29),
        to: now,
    };
    let shell = Window {
        from: now - chrono::Duration::seconds(2),
        to: now + chrono::Duration::seconds(5),
    };
    let both = edit.span(shell);
    assert_eq!(both.from, edit.from);
    assert_eq!(both.to, shell.to);
}

#[test]
fn has_evidence_without_an_edit_or_a_command_naming_the_change_is_false() {
    let runs = vec![run(3), run(7)];
    let facts = Facts {
        path: "src/a.rs",
        calls: &[],
        runs: &runs,
        after: None,
        changed: now_window(),
    };
    assert!(!has_evidence(&[3, 7], &facts));
}

#[test]
fn has_evidence_an_edit_by_a_candidate_is_true_and_by_a_stranger_is_false() {
    let calls = vec![edit(3, "alpha")];
    let facts = Facts {
        path: "src/a.rs",
        calls: &calls,
        runs: &[],
        after: None,
        changed: now_window(),
    };
    assert!(has_evidence(&[3, 7], &facts));
    assert!(!has_evidence(&[7, 9], &facts));
}

#[test]
fn has_evidence_a_command_naming_the_checkout_or_the_path_is_true() {
    let checkout = vec![ShellRun {
        names_checkout: true,
        ..run(7)
    }];
    let path = vec![ShellRun {
        command: Some("sed -i s/a/b/ src/a.rs".to_owned()),
        ..run(7)
    }];
    for runs in [&checkout, &path] {
        let facts = Facts {
            path: "src/a.rs",
            calls: &[],
            runs,
            after: None,
            changed: now_window(),
        };
        assert!(has_evidence(&[3, 7], &facts));
    }
}
