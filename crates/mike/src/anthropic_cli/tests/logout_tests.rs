use super::fake_cli::{FakeCli, LOGOUT_FAILS_SCRIPT, LOGOUT_OK_SCRIPT};
use crate::anthropic_cli::AnthropicCli;
use crate::anthropic_cli::logout::run_logout;

#[tokio::test]
async fn run_logout_ok_when_cli_exits_success() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGOUT_OK_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());

    run_logout(&cli).await?;
    Ok(())
}

#[tokio::test]
async fn run_logout_surfaces_failure_message() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGOUT_FAILS_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());

    let Err(err) = run_logout(&cli).await else {
        anyhow::bail!("run_logout must fail when the CLI exits non-zero");
    };
    assert!(err.to_string().contains("logout"));
    Ok(())
}
