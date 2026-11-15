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

fn picked(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        WHO.to_owned(),
        Answer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
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
    let changed = now_window();
    let facts = Facts {
        path: "/work/alpha",
        calls: &[],
        runs: &runs,
        after: None,
        changed,
    };
    let state = state(&[3, 7], &facts);
    assert_eq!(
        state["candidate_sessions"][1]["shell_commands_running_in_the_checkout"][0]["command"],
        "git worktree remove .claude/worktrees/x"
    );
    assert_eq!(state["changed_between"]["from"], json!(changed.from));
    assert_eq!(state["changed_between"]["to"], json!(changed.to));
}

#[tokio::test]
async fn resolve_on_an_answered_choice_names_the_session() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub_jev(200, choice_body(WHO, "s7", &["s3", "unknown"])).await;
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
fn state_each_edit_carries_the_window_it_ran_in() {
    let calls = vec![edit(3, "alpha")];
    let facts = Facts {
        path: "src/a.rs",
        calls: &calls,
        runs: &[],
        after: None,
        changed: now_window(),
    };
    let state = state(&[3, 7], &facts);
    let first = &state["candidate_sessions"][0]["edit_tool_calls_naming_the_file"][0];
    assert_eq!(first["ran_from"], json!(calls[0].ran_from));
    assert_eq!(first["ran_to"], json!(calls[0].ran_to));
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
