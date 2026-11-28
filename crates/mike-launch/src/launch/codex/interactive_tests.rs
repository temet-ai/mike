use super::*;
use crate::launch::backend::fake::FakeBackend;
use crate::launch::cli::{open, test_kit};
use crate::launch::{AssembleOptions, BackendId, split_launch_args};

fn spec(dir: &Path, args: &[&str]) -> anyhow::Result<LaunchSpec> {
    let mut tokens = test_kit::toks(&["--runtime", "codex"]);
    tokens.extend(test_kit::toks(args));
    Ok(LaunchSpec::assemble(
        split_launch_args(tokens)?,
        None,
        &AssembleOptions {
            program: "claude".into(),
            proxy: None,
            brief_inline_max: 1024,
            briefs_dir: dir,
            brief_notes: false,
            brief_by_mail: false,
        },
    )?)
}

#[test]
fn configuration_preserves_prompt_and_custom_exporter_without_claiming_observation()
-> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let mut spec = spec(home.path(), &["-m", "gpt-fixture", "hello"])?;
    configure(&mut spec, home.path(), Some("127.0.0.1:9988"))?;
    assert_eq!(spec.args.last().map(String::as_str), Some("hello"));
    let session = spec
        .session
        .ok_or_else(|| anyhow::anyhow!("configure left the launch without a session"))?;
    assert!(spec.args.contains(&format!(
        "openai_base_url=\"http://127.0.0.1:9988/codex/launch/{session}\""
    )));
    assert!(!spec.args.iter().any(|arg| arg.contains("otel.exporter")));
    assert_eq!(spec.env.get("CODEX_HOME"), spec.env.get("MIKE_CODEX_HOME"));
    let output = LaunchOutput {
        backend: BackendId::Herdr,
        workspace: None,
        folder: "/repo".into(),
        surface: None,
        session: None,
        reused: false,
        adopted: false,
        opened: false,
        delivered: None,
        confirmed: false,
        channel_check: ChannelCheck::NoSessionId,
        brief_mode: None,
        run: None,
        seat: None,
        reason: None,
    };
    let output = opened(output, &spec, &FakeBackend::surface(None));
    assert!(output.opened);
    assert!(!output.confirmed);
    assert_eq!(output.delivered, None);
    assert_eq!(output.exit_code(), 0);
    assert_eq!(output.channel_check, ChannelCheck::Disabled);
    Ok(())
}

#[test]
fn proxy_opt_out_preserves_arguments_without_exporter_injection() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let mut spec = spec(home.path(), &["--no-proxy-env", "--", "--", "resume"])?;
    configure(&mut spec, home.path(), None)?;
    let delimiter = spec
        .args
        .iter()
        .position(|arg| arg == "--")
        .ok_or_else(|| anyhow::anyhow!("no provider delimiter"))?;
    assert!(
        !spec.args[..delimiter]
            .iter()
            .any(|arg| arg.contains("otel.exporter"))
    );
    assert!(
        !spec
            .args
            .iter()
            .any(|arg| arg.starts_with("openai_base_url="))
    );
    assert_eq!(spec.args.last().map(String::as_str), Some("resume"));
    spec.flags.proxy_env = true;
    assert!(configure(&mut spec, home.path(), None).is_err());
    Ok(())
}

#[tokio::test]
async fn codex_launch_never_writes_claude_trust_and_requires_a_durable_ledger() -> anyhow::Result<()>
{
    let home = tempfile::tempdir()?;
    let mut backend = FakeBackend::available();
    backend.backend_id = Some(BackendId::HerdrCore);
    let config = test_kit::quiet_config();
    let runner = test_kit::runner(&backend, &config, home.path());
    std::fs::write(runner.trust.config_path(), "{\"projects\":{}}")?;
    let folder = test_kit::project_dir(home.path());
    let prepared = open::prepare(
        &runner,
        test_kit::open_tokens(&folder, &["--runtime", "codex", "-m", "gpt-fixture"]),
        &mut std::io::empty(),
    )
    .await
    .map_err(|error| anyhow::anyhow!("{:?}", error))?;
    assert_eq!(
        std::fs::read_to_string(runner.trust.config_path())?,
        "{\"projects\":{}}"
    );
    let original = prepared.split.claude_args.clone();
    let combined = open::with_operator_args(
        prepared.split.clone(),
        &test_kit::toks(&["--dangerously-skip-permissions"]),
    );
    assert_eq!(combined.claude_args, original);
    let error = match open::start(&runner, prepared).await {
        Ok(_) => anyhow::bail!("Codex opened without its ledger"),
        Err(error) => error,
    };
    assert!(
        error
            .error
            .to_string()
            .contains("durable Mike launch ledger")
    );
    assert!(backend.open_requests.lock().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn codex_templates_are_refused_without_opening_or_resolving_a_ledger() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let mut backend = FakeBackend::available();
    backend.backend_id = Some(BackendId::HerdrCore);
    let config = test_kit::quiet_config();
    let runner = test_kit::runner(&backend, &config, home.path());
    let folder = test_kit::project_dir(home.path());
    let warm = crate::launch::cli::warm::execute(
        &runner,
        test_kit::open_tokens(&folder, &["--runtime", "codex", "--name", "fixture"]),
        &mut std::io::empty(),
    )
    .await;
    let error = match warm {
        Ok(_) => anyhow::bail!("Codex warm was accepted"),
        Err(error) => error,
    };
    assert!(
        error
            .error
            .to_string()
            .contains("warm templates are Claude-only")
    );
    let fork = crate::launch::cli::fork::execute(
        &runner,
        test_kit::toks(&["missing-template", "--runtime", "codex"]),
        &mut std::io::empty(),
    )
    .await;
    let error = fork
        .map_err(|error| error.error)
        .err()
        .ok_or_else(|| anyhow::anyhow!("Codex fork was accepted"))?;
    assert!(error.to_string().contains("template forks are Claude-only"));
    assert!(backend.open_requests.lock().await.is_empty());
    Ok(())
}
