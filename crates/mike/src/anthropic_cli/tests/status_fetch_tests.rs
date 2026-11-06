use super::fake_cli::{FakeCli, LOGGED_IN_SCRIPT, LOGGED_OUT_SCRIPT};
use crate::anthropic_cli::AnthropicCli;
use crate::anthropic_cli::status::fetch_cli_status;

#[tokio::test]
async fn fetch_cli_status_signed_in_carries_account_fields() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGGED_IN_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());

    let dto = fetch_cli_status(&cli).await;

    assert!(dto.installed);
    assert!(dto.logged_in);
    assert_eq!(dto.email.as_deref(), Some("dawid@example.com"));
    assert_eq!(dto.subscription_type.as_deref(), Some("max"));
    assert_eq!(dto.version.as_deref(), Some("2.1.280 (Claude Code)"));
    Ok(())
}

#[tokio::test]
async fn fetch_cli_status_signed_out_has_no_account_fields() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGGED_OUT_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());

    let dto = fetch_cli_status(&cli).await;

    assert!(dto.installed);
    assert!(!dto.logged_in);
    assert!(dto.email.is_none());
    Ok(())
}

#[tokio::test]
async fn fetch_cli_status_missing_binary_reports_not_installed() {
    let cli = AnthropicCli::with_binary("/nonexistent/definitely-not-claude-cli-binary");

    let dto = fetch_cli_status(&cli).await;

    assert!(!dto.installed);
    assert!(!dto.logged_in);
    assert!(dto.error.is_some());
}
