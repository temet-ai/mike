use super::*;

#[test]
fn a_service_that_does_not_answer_fails_with_the_service_step() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.services[0].answering = false;
    let checks = evaluate(&observed);
    let proxy = check(&checks, "proxy_port")?;
    assert_eq!(proxy.status, Status::Fail);
    assert_eq!(proxy.setup_step, Some(SetupStep::Service));
    assert!(proxy.detail.contains("127.0.0.1:9988"), "{}", proxy.detail);
    Ok(())
}

#[test]
fn the_web_ui_port_being_down_is_only_a_warning_and_a_switched_off_one_is_fine()
-> anyhow::Result<()> {
    let mut observed = healthy();
    observed.services[2].answering = false;
    assert_eq!(
        check(&evaluate(&observed), "spa_port")?.status,
        Status::Warn
    );
    observed.services[2].enabled = false;
    assert_eq!(check(&evaluate(&observed), "spa_port")?.status, Status::Ok);
    Ok(())
}

#[test]
fn a_proxy_running_a_different_build_is_a_warning_naming_both_builds() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.services[1].running_sha = Some("fedcba9876543210".to_string());
    let checks = evaluate(&observed);
    let inspect = check(&checks, "inspect_port")?;
    assert_eq!(inspect.status, Status::Warn);
    assert!(
        inspect.detail.contains("fedcba9") && inspect.detail.contains("0123456"),
        "{}",
        inspect.detail
    );
    Ok(())
}

#[test]
fn a_binary_without_the_web_ui_fails_only_when_the_web_ui_is_enabled() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.spa = SpaFacts {
        enabled: true,
        embedded: false,
    };
    assert_eq!(
        check(&evaluate(&observed), "spa_built")?.status,
        Status::Fail
    );
    observed.spa.enabled = false;
    assert_eq!(check(&evaluate(&observed), "spa_built")?.status, Status::Ok);
    Ok(())
}

#[test]
fn the_sidecar_and_its_certificate_are_warnings_when_missing() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.sidecar.reachable = Err("connection refused".to_string());
    observed.sidecar.ca_found = None;
    let checks = evaluate(&observed);
    assert_eq!(check(&checks, "sidecar")?.status, Status::Warn);
    let ca = check(&checks, "sidecar_ca")?;
    assert_eq!(ca.status, Status::Warn);
    assert!(ca.detail.contains("ca-cert.pem"), "{}", ca.detail);
    assert_eq!(ca.fix.as_deref(), Some("mike setup service"));
    Ok(())
}

#[test]
fn claude_missing_fails_present_passes_and_a_sandbox_skips_only_the_version() -> anyhow::Result<()>
{
    let mut observed = healthy();
    observed.claude = ToolFacts {
        path: None,
        version: None,
        version_skipped: false,
    };
    let missing = evaluate(&observed);
    assert_eq!(check(&missing, "claude")?.status, Status::Fail);
    assert_eq!(
        check(&missing, "claude")?.fix.as_deref(),
        Some("mike setup claude")
    );
    observed.claude = ToolFacts {
        path: Some(PathBuf::from("/bin/claude")),
        version: None,
        version_skipped: true,
    };
    let sandboxed = evaluate(&observed);
    assert_eq!(check(&sandboxed, "claude")?.status, Status::Ok);
    assert!(check(&sandboxed, "claude")?.detail.contains("MIKE_SANDBOX"));
    Ok(())
}

fn registration(command: Option<&str>, exists: bool, project: bool) -> Registration {
    Registration {
        command: command.map(str::to_string),
        command_exists: exists,
        inspect_url: None,
        in_a_project: project,
    }
}

#[test]
fn mike_mcp_missing_or_unregistered_warns_with_the_claude_step() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.mcp = McpFacts {
        binary: None,
        registration: Ok(registration(None, false, false)),
    };
    let checks = evaluate(&observed);
    for id in ["mcp_binary", "mcp_registered"] {
        let found = check(&checks, id)?;
        assert_eq!(found.status, Status::Warn, "{id}");
        assert_eq!(found.setup_step, Some(SetupStep::Claude), "{id}");
    }
    observed.mcp.registration = Err("unreadable".to_string());
    assert_eq!(
        check(&evaluate(&observed), "mcp_registered")?.status,
        Status::Warn
    );
    Ok(())
}

#[test]
fn a_registration_pointing_at_a_missing_file_warns_and_names_the_file() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.mcp.registration = Ok(registration(Some("/gone/mike-mcp"), false, false));
    let found = check(&evaluate(&observed), "mcp_registered")?.clone();
    assert_eq!(found.status, Status::Warn);
    assert!(found.detail.contains("/gone/mike-mcp"), "{}", found.detail);
    assert_eq!(found.setup_step, Some(SetupStep::Claude));
    Ok(())
}

#[test]
fn a_project_registration_alone_counts_as_registered() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.mcp.registration = Ok(registration(None, false, true));
    assert_eq!(
        check(&evaluate(&observed), "mcp_registered")?.status,
        Status::Ok
    );
    Ok(())
}

#[test]
fn the_route_and_the_shell_function_warn_until_chosen_and_written() -> anyhow::Result<()> {
    let ok = evaluate(&healthy());
    assert_eq!(check(&ok, "claude_route")?.status, Status::Ok);
    assert_eq!(check(&ok, "shell_function")?.status, Status::Ok);
    assert!(check(&ok, "shell_function")?.detail.contains("zsh"));
    let mut observed = healthy();
    observed.connection.route = None;
    observed.connection.shells[0].block = Ok(false);
    let warned = evaluate(&observed);
    for id in ["claude_route", "shell_function"] {
        let found = check(&warned, id)?;
        assert_eq!(found.status, Status::Warn, "{id}");
        assert_eq!(found.setup_step, Some(SetupStep::Claude), "{id}");
        assert_eq!(found.fix.as_deref(), Some("mike setup claude"), "{id}");
    }
    Ok(())
}

#[test]
fn background_models_warn_when_no_provider_exists_and_never_print_a_key() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.models.claude_on_path = false;
    let none = evaluate(&observed);
    let models = check(&none, "models")?;
    assert_eq!(models.status, Status::Warn);
    assert_eq!(models.setup_step, Some(SetupStep::Models));
    observed.models.openrouter_key_variable = Some("OPENROUTER_API_KEY".to_string());
    observed.models.codex_on_path = true;
    let found = evaluate(&observed);
    let models = check(&found, "models")?;
    assert_eq!(models.status, Status::Ok);
    assert!(models.detail.contains("OPENROUTER_API_KEY") && models.detail.contains("codex"));
    Ok(())
}

#[test]
fn degraded_or_inert_capabilities_warn_and_name_them() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.models.capabilities = Some(vec![
        CapabilityFacts {
            id: "steering".to_string(),
            label: "Steering & judges".to_string(),
            state: "inert".to_string(),
        },
        CapabilityFacts {
            id: "persistence".to_string(),
            label: "Persistence".to_string(),
            state: "active".to_string(),
        },
    ]);
    let checks = evaluate(&observed);
    let capabilities = check(&checks, "capabilities")?;
    assert_eq!(capabilities.status, Status::Warn);
    assert_eq!(capabilities.setup_step, Some(SetupStep::Features));
    assert!(capabilities.detail.contains("Steering & judges is inert"));
    observed.models.capabilities = None;
    assert!(evaluate(&observed).iter().all(|c| c.id != "capabilities"));
    Ok(())
}

#[test]
fn the_embedding_server_is_fine_unconfigured_and_warns_when_configured_but_silent()
-> anyhow::Result<()> {
    let mut observed = healthy();
    observed.embeddings = EmbeddingFacts {
        url: None,
        reachable: None,
    };
    assert_eq!(
        check(&evaluate(&observed), "embeddings")?.status,
        Status::Ok
    );
    observed.embeddings = EmbeddingFacts {
        url: Some("http://127.0.0.1:7997".to_string()),
        reachable: Some(false),
    };
    let checks = evaluate(&observed);
    let embeddings = check(&checks, "embeddings")?;
    assert_eq!(embeddings.status, Status::Warn);
    assert_eq!(embeddings.fix.as_deref(), Some("mike setup embeddings"));
    Ok(())
}

#[test]
fn no_launch_backend_warns_with_the_terminal_step() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.launch_backend = "none".to_string();
    let checks = evaluate(&observed);
    let backend = check(&checks, "launch_backend")?;
    assert_eq!(backend.status, Status::Warn);
    assert_eq!(backend.fix.as_deref(), Some("mike setup terminal"));
    Ok(())
}
