use std::time::Duration;

use super::super::error::ClaudeCodeError;
use super::super::probe::tool_count_unguarded;
use super::Machine;

const URL: &str = "http://127.0.0.1:1";

pub(super) const ANSWERING_SCRIPT: &str = r#"#!/bin/sh
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"fake","version":"0"}}}'
read notification
read list
printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/message","params":{}}'
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"a"},{"name":"b"},{"name":"c"}]}}'
"#;

#[tokio::test]
async fn a_server_that_answers_has_its_tools_counted() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let binary = machine.add_executable("mike-mcp", ANSWERING_SCRIPT)?;
    let count = tool_count_unguarded(&binary, URL, Duration::from_secs(10)).await?;
    assert_eq!(count, 3);
    Ok(())
}

#[tokio::test]
async fn a_server_that_exits_at_once_is_reported() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let binary = machine.add_executable("mike-mcp", "#!/bin/sh\nexit 3\n")?;
    let error = tool_count_unguarded(&binary, URL, Duration::from_secs(10))
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(matches!(error, ClaudeCodeError::Probe(_)), "{error}");
    Ok(())
}

#[tokio::test]
async fn a_server_that_stays_silent_times_out() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let binary = machine.add_executable("mike-mcp", "#!/bin/sh\nsleep 30\n")?;
    let error = tool_count_unguarded(&binary, URL, Duration::from_millis(300))
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(error.to_string().contains("no answer"), "{error}");
    Ok(())
}

#[tokio::test]
async fn a_missing_binary_is_reported_with_its_path() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let missing = machine.home().join("nope");
    let error = tool_count_unguarded(&missing, URL, Duration::from_secs(1))
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(error.to_string().contains("could not start"), "{error}");
    Ok(())
}

#[tokio::test]
async fn an_error_answer_to_initialize_is_reported() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let binary = machine.add_executable(
        "mike-mcp",
        "#!/bin/sh\nread init\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-1,\"message\":\"no\"}}'\n",
    )?;
    let error = tool_count_unguarded(&binary, URL, Duration::from_secs(10))
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(error.to_string().contains("error"), "{error}");
    Ok(())
}
