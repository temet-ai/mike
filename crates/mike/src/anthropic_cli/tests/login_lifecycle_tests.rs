use super::fake_cli::{
    FakeCli, LOGIN_FAILS_ON_ANY_CODE_SCRIPT, LOGIN_HANGS_SCRIPT, LOGIN_SUCCEEDS_ON_ANY_CODE_SCRIPT,
    LOGIN_URL,
};
use crate::anthropic_cli::AnthropicCli;
use crate::anthropic_cli::login::{LoginPhase, spawn_login};
use crate::anthropic_cli::registry::LoginRegistry;

async fn await_phase(session: &crate::anthropic_cli::login::LoginSession) -> LoginPhase {
    for _ in 0..200 {
        let phase = session.status().await.phase;
        if matches!(phase, LoginPhase::AwaitingCode { .. }) || phase.is_terminal() {
            return phase;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    session.status().await.phase
}

async fn await_terminal(session: &crate::anthropic_cli::login::LoginSession) -> LoginPhase {
    for _ in 0..200 {
        let phase = session.status().await.phase;
        if phase.is_terminal() {
            return phase;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    session.status().await.phase
}

#[tokio::test]
async fn spawn_login_captures_url_then_succeeds_after_code() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGIN_SUCCEEDS_ON_ANY_CODE_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());

    let session = spawn_login(&cli)?;
    let awaiting = await_phase(&session).await;
    assert_eq!(
        awaiting,
        LoginPhase::AwaitingCode {
            url: LOGIN_URL.to_owned()
        }
    );

    session.submit_code("123456").await?;
    let terminal = await_terminal(&session).await;
    assert_eq!(terminal, LoginPhase::Succeeded);
    Ok(())
}

#[tokio::test]
async fn spawn_login_reports_failed_when_cli_rejects_code() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGIN_FAILS_ON_ANY_CODE_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());

    let session = spawn_login(&cli)?;
    await_phase(&session).await;
    session.submit_code("wrong").await?;

    let terminal = await_terminal(&session).await;
    assert!(
        matches!(terminal, LoginPhase::Failed { .. }),
        "{terminal:?}"
    );
    Ok(())
}

#[tokio::test]
async fn cancel_kills_hanging_login_process() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGIN_HANGS_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());

    let session = spawn_login(&cli)?;
    await_phase(&session).await;
    session.cancel();

    let terminal = await_terminal(&session).await;
    assert_eq!(terminal, LoginPhase::Cancelled);
    Ok(())
}

#[tokio::test]
async fn spawn_login_missing_binary_returns_not_found() {
    let cli = AnthropicCli::with_binary("/nonexistent/definitely-not-claude-cli-binary");
    let result = spawn_login(&cli);
    assert!(matches!(
        result,
        Err(crate::anthropic_cli::AnthropicCliError::NotFound)
    ));
}

#[tokio::test]
async fn registry_refuses_second_login_while_first_still_running() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGIN_HANGS_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());
    let registry = LoginRegistry::new();

    let first = registry.start(&cli).await?;
    let second = registry.start(&cli).await;

    assert!(matches!(
        second,
        Err(crate::anthropic_cli::AnthropicCliError::AlreadyRunning(_))
    ));

    registry.cancel(&first.id).await?;
    Ok(())
}

#[tokio::test]
async fn registry_allows_new_login_after_previous_one_finished() -> anyhow::Result<()> {
    let fake = FakeCli::new(LOGIN_SUCCEEDS_ON_ANY_CODE_SCRIPT)?;
    let cli = AnthropicCli::with_binary(fake.bin());
    let registry = LoginRegistry::new();

    let first = registry.start(&cli).await?;
    registry.submit_code(&first.id, "123456").await?;
    for _ in 0..200 {
        if registry.status(&first.id).await?.phase.is_terminal() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let second = registry.start(&cli).await?;
    assert_ne!(second.id, first.id);
    Ok(())
}
