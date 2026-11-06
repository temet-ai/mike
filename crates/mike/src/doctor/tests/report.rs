use super::*;

#[test]
fn a_healthy_machine_passes_every_check() -> anyhow::Result<()> {
    let checks = evaluate(&healthy());
    let report = Report::new(checks);
    assert!(report.passed, "{}", render::text(&report));
    assert_eq!(report.fail, 0);
    assert_eq!(report.warn, 0);
    assert_eq!(report.exit_code(), 0);
    assert!(report.checks.iter().all(|c| c.fix.is_none()));
    Ok(())
}

#[test]
fn every_check_the_audit_lists_is_present_with_a_stable_id() {
    let ids: Vec<_> = evaluate(&healthy()).iter().map(|c| c.id).collect();
    for expected in [
        "config_file",
        "version",
        "database_url",
        "database_reachable",
        "postgres_version",
        "database_extensions",
        "database_migrations",
        "proxy_port",
        "inspect_port",
        "spa_port",
        "spa_built",
        "sidecar",
        "sidecar_ca",
        "claude",
        "mcp_binary",
        "mcp_registered",
        "claude_route",
        "shell_function",
        "models",
        "capabilities",
        "embeddings",
        "launch_backend",
    ] {
        assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
    }
}

#[test]
fn config_file_states_map_to_ok_warn_and_fail() -> anyhow::Result<()> {
    let status = |state: FileState| -> anyhow::Result<Status> {
        let mut observed = healthy();
        observed.config_file.state = state;
        Ok(check(&evaluate(&observed), "config_file")?.status)
    };
    assert_eq!(status(FileState::Loaded { keys: 1 })?, Status::Ok);
    assert_eq!(status(FileState::Missing)?, Status::Warn);
    assert_eq!(status(FileState::Unusable("bad".into()))?, Status::Fail);
    assert_eq!(
        status(FileState::NoLocation("no HOME".into()))?,
        Status::Fail
    );
    Ok(())
}

#[test]
fn warnings_alone_exit_zero_and_one_failure_exits_one() {
    let warn_only = Report::new(vec![
        Check::ok("a", "a", "fine"),
        Check::warn("b", "b", "meh", Some(SetupStep::Models)),
    ]);
    assert!(warn_only.passed);
    assert_eq!(warn_only.exit_code(), 0);
    let failing = Report::new(vec![Check::fail("c", "c", "broken", None)]);
    assert!(!failing.passed);
    assert_eq!(failing.exit_code(), 1);
}

#[test]
fn every_setup_step_has_the_documented_name() {
    let names: Vec<_> = [
        SetupStep::Database,
        SetupStep::Service,
        SetupStep::Claude,
        SetupStep::Models,
        SetupStep::Embeddings,
        SetupStep::Terminal,
        SetupStep::Features,
    ]
    .iter()
    .map(|s| s.command())
    .collect();
    assert_eq!(
        names,
        [
            "mike setup database",
            "mike setup service",
            "mike setup claude",
            "mike setup models",
            "mike setup embeddings",
            "mike setup terminal",
            "mike setup features",
        ]
    );
}

#[test]
fn text_output_shows_the_fix_under_each_non_ok_check_and_a_totals_line() {
    let report = Report::new(vec![
        Check::ok("a", "alpha", "fine"),
        Check::fail("b", "beta", "broken", Some(SetupStep::Database)),
    ]);
    let text = render::text(&report);
    assert!(text.contains("ok    alpha  fine"), "{text}");
    assert!(text.contains("FAIL  beta   broken"), "{text}");
    assert!(text.contains("fix: mike setup database"), "{text}");
    assert!(text.contains("1 ok, 0 warnings, 1 failures"), "{text}");
}

#[test]
fn json_output_carries_status_detail_and_the_fix_command() -> anyhow::Result<()> {
    let report = Report::new(vec![Check::warn(
        "models",
        "background models",
        "none",
        Some(SetupStep::Models),
    )]);
    let value: Value = serde_json::from_str(&render::json(&report))?;
    assert_eq!(value["passed"], true);
    assert_eq!(value["warn"], 1);
    let first = &value["checks"][0];
    assert_eq!(first["id"], "models");
    assert_eq!(first["status"], "warn");
    assert_eq!(first["detail"], "none");
    assert_eq!(first["fix"], "mike setup models");
    assert_eq!(first["setup_step"], "models");
    Ok(())
}
