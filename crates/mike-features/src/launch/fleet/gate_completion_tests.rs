use super::*;
use crate::launch::fleet::tests::changed;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use serde_json::json;

#[test]
fn completion_names_exact_command_commit_verdict_and_log() {
    let log = Path::new("/project/.claude/worktrees/mike-gate-abcdef-output.log");
    for (code, verdict) in [
        (Some(0), "green"),
        (Some(3), "failed (exit 3)"),
        (None, "not completed"),
    ] {
        let text = completion_mail(
            "old gate",
            "abcdef0123456789",
            code,
            "FAIL binary example",
            log,
        );
        for expected in [
            "old gate",
            "abcdef0123456789",
            verdict,
            log.to_str().unwrap_or_default(),
        ] {
            assert!(text.contains(expected), "{text}");
        }
        assert_eq!(text.contains("FAIL binary example"), code != Some(0));
    }
}

#[tokio::test]
async fn automatic_gate_completion_reaches_acceptance_on_green_failure_and_no_exit_code()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let acceptance = Uuid::new_v4();
    persist
        .launch()
        .ensure_session_exists(&acceptance.to_string())
        .await?;
    let dir = tempfile::tempdir()?;
    std::fs::create_dir_all(dir.path().join(".claude/worktrees"))?;
    let def = changed(json!({"repo": dir.path().to_string_lossy(),
        "coordinator": {"session": Uuid::new_v4().to_string()},
        "acceptance": {"session": acceptance.to_string()}, "gate": {"command": "old gate"}}));
    for (commit, code) in [("aaaa", Some(0)), ("bbbb", Some(1)), ("cccc", None)] {
        let target = Target {
            commit: commit.to_owned(),
            pushed: true,
            slot: dir.path().to_owned(),
        };
        let log = completion_log(&target, &def.gate.command, commit);
        std::fs::write(&log, "test output")?;
        after_run(
            &persist,
            &def,
            "automatic-completion",
            &target,
            (code, "test output", &log, true),
        )
        .await;
        assert_eq!(std::fs::read_to_string(log)?, "test output");
    }
    let inbox = persist
        .launch()
        .fetch_for_session_with_activity(acceptance)
        .await?
        .inbox;
    for commit in ["aaaa", "bbbb", "cccc"] {
        assert_eq!(
            inbox
                .iter()
                .filter(|m| m.row.body.contains(&format!("at commit {commit}:")))
                .count(),
            1
        );
    }
    db.cleanup().await?;
    Ok(())
}

#[test]
fn completion_logs_preserve_different_commands_at_the_same_commit() {
    let target = Target {
        commit: "abcdef".into(),
        pushed: true,
        slot: "/project".into(),
    };
    assert_ne!(
        completion_log(&target, "first gate", "1"),
        completion_log(&target, "second gate", "1")
    );
    assert_eq!(
        completion_log(&target, "first gate", "1"),
        completion_log(&target, "first gate", "1")
    );
}

#[test]
fn repeated_same_command_at_same_commit_has_distinct_execution_logs() {
    let target = Target {
        commit: "abc".into(),
        pushed: true,
        slot: "/project".into(),
    };
    assert_ne!(
        completion_log(&target, "gate", "1"),
        completion_log(&target, "gate", "2")
    );
}

#[test]
fn completion_mail_retains_all_observed_failures_without_full_noise() {
    let mut output = (0..75)
        .map(|i| format!("test suite::case_{i} ... FAILED\n"))
        .collect::<String>();
    output.push_str(&"successful noise\n".repeat(100));
    let mail = completion_mail("gate", "abc", None, &output, Path::new("/exact/run.log"));
    assert!(mail.contains("not completed"));
    assert!(mail.contains("suite::case_0"));
    assert!(mail.contains("suite::case_74"));
    assert!(mail.contains("75 unique"));
    assert!(!mail.contains("successful noise"));
}
