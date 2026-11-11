use std::path::PathBuf;

use serde_json::json;
use uuid::Uuid;

use super::seat::absorb;
use super::*;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn options() -> SeatOptions {
    SeatOptions {
        program: "codex".into(),
        cwd: PathBuf::from("/repo"),
        codex_home: PathBuf::from("/state/seats/repo/codex/run-1"),
        model: Some("gpt-6.1-sol".into()),
        effort: Some("high".into()),
        developer_instructions: "contract".into(),
        mcp: Some(McpBinding {
            command: "mike-mcp".into(),
            env: vec![
                ("MIKE_MCP_PROFILE".into(), "worker".into()),
                ("CLAUDE_CODE_SESSION_ID".into(), "s-1".into()),
            ],
        }),
    }
}

#[test]
fn codex_model_check_refuses_claude_names_and_accepts_codex_ids() {
    for name in ["opus", "sonnet", "claude-opus-5-5", "haiku", "fable"] {
        assert!(model_problem(name).is_some(), "{name} should be refused");
    }
    for name in ["gpt-6.1-sol", "gpt-5.6-luna", ""] {
        assert_eq!(model_problem(name), None, "{name} should pass");
    }
    assert!(model_problem("gpt 6").is_some());
}

#[test]
fn codex_effort_and_agent_checks_name_the_claude_only_values() {
    assert_eq!(effort_problem(Some("xhigh")), None);
    assert_eq!(effort_problem(None), None);
    assert!(effort_problem(Some("max")).is_some_and(|p| p.contains("minimal")));
    assert!(agent_problem("rust-developer").is_some_and(|p| p.contains("--agent")));
    assert_eq!(agent_problem(""), None);
}

#[test]
fn thread_start_params_carry_seat_policy_and_mike_binding_without_claude_flags() {
    let params = options().thread_start();
    assert_eq!(params["sandbox"], "workspace-write");
    assert_eq!(params["approvalPolicy"], "never");
    assert_eq!(params["ephemeral"], false);
    assert_eq!(params["model"], "gpt-6.1-sol");
    assert_eq!(params["developerInstructions"], "contract");
    let mike = &params["config"]["mcp_servers"]["mike"];
    assert_eq!(mike["command"], "mike-mcp");
    assert_eq!(mike["env"]["CLAUDE_CODE_SESSION_ID"], "s-1");
    assert_eq!(mike["env"]["MIKE_MCP_PROFILE"], "worker");
    let text = params.to_string();
    for flag in [
        "--model",
        "--agent",
        "--settings",
        "--append-system-prompt",
        "--effort",
    ] {
        assert!(!text.contains(flag), "{flag} leaked into {text}");
    }
}

#[test]
fn resume_fork_and_turn_params_name_the_thread() {
    let seat = options();
    assert_eq!(seat.thread_resume("t-1")["threadId"], "t-1");
    assert_eq!(seat.thread_fork("t-1")["threadId"], "t-1");
    assert_eq!(seat.thread_fork("t-1")["ephemeral"], false);
    let turn = seat.turn_start("t-1", "brief");
    assert_eq!(turn["input"][0]["text"], "brief");
    assert_eq!(turn["effort"], "high");
}

#[test]
fn contract_is_rendered_per_seat_kind_with_the_seat_identity() {
    let session = Uuid::new_v4();
    for (profile, kind) in [
        ("coordinator", SeatKind::Coordinator),
        ("worker", SeatKind::Worker),
        ("observer", SeatKind::Observer),
        ("acceptance", SeatKind::Acceptance),
    ] {
        assert_eq!(SeatKind::from_profile(profile), kind);
        let text = render_contract(kind, "rust-P1", session, "Do the task.");
        assert!(text.contains(&format!("the {profile} of a mike fleet")));
        assert!(text.contains(&session.to_string()));
        assert!(text.contains("mcp__mike__<name>"));
        assert!(text.ends_with("Do the task.\n"));
    }
}

#[test]
fn namespace_is_stable_per_codex_home() {
    let home = PathBuf::from("/state/codex/run-1");
    assert_eq!(namespace_for(&home), namespace_for(&home));
    assert_ne!(namespace_for(&home).id, namespace_for(&home.join("x")).id);
    assert!(namespace_for(&home).validate().is_ok());
    assert_eq!(
        seat_codex_home(&PathBuf::from("/s"), "fleet run/1"),
        PathBuf::from("/s/codex/fleet_run_1")
    );
}

#[test]
fn turn_outcome_collects_final_answer_and_commands_of_its_own_thread() {
    let mut outcome = TurnOutcome {
        turn_id: "u-1".into(),
        ..TurnOutcome::default()
    };
    let other = json!({"method": "turn/completed", "params": {"threadId": "t-2", "turn": {"id": "u-1", "status": "completed"}}});
    assert!(!absorb(&mut outcome, &other, "t-1"));
    let command = json!({"method": "item/completed", "params": {"threadId": "t-1", "item": {"type": "commandExecution", "command": "./check.sh", "exitCode": 0}}});
    let answer = json!({"method": "item/completed", "params": {"threadId": "t-1", "item": {"type": "agentMessage", "phase": "final_answer", "text": "done"}}});
    let done = json!({"method": "turn/completed", "params": {"threadId": "t-1", "turn": {"id": "u-1", "status": "completed"}}});
    assert!(!absorb(&mut outcome, &command, "t-1"));
    assert!(!absorb(&mut outcome, &answer, "t-1"));
    assert!(absorb(&mut outcome, &done, "t-1"));
    assert_eq!(outcome.final_text, "done");
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.commands, vec![("./check.sh".to_string(), Some(0))]);
}

#[tokio::test]
async fn seat_thread_binding_survives_a_restart_and_keeps_fork_ancestry() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        anyhow::bail!("database tests need MIKE_TEST_ADMIN_URL");
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let home = PathBuf::from(format!("/state/codex/{}", Uuid::new_v4()));
    let first = Uuid::new_v4();
    let forked = Uuid::new_v4();
    bind_seat_thread(&persist, &home, first, "thread-A", None).await?;
    bind_seat_thread(&persist, &home, first, "thread-A", None).await?;
    bind_seat_thread(&persist, &home, forked, "thread-B", Some("thread-A")).await?;
    let restarted = PersistHandle::connect(&db.database_url).await?;
    assert_eq!(
        super::register::bound_thread(&restarted, &home, first).await?,
        Some("thread-A".to_string())
    );
    let binding = restarted
        .runtime_thread_by_session(forked)
        .await?
        .ok_or_else(|| anyhow::anyhow!("fork binding missing"))?;
    assert_eq!(
        binding.forked_from.map(|f| f.thread_id),
        Some("thread-A".into())
    );
    assert_eq!(
        super::register::bound_thread(&restarted, &home.join("other"), first).await?,
        None
    );
    assert!(
        bind_seat_thread(&persist, &home, first, "thread-C", None)
            .await
            .is_err()
    );
    db.cleanup().await?;
    Ok(())
}
