use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::common::{
    BINARY, call_list_requests_tool, initialize_request, initialized_notification,
    list_tools_request, read_next_message,
};

#[tokio::test]
async fn initialize_handshake_returns_server_info_with_all_tools() -> anyhow::Result<()> {
    let mut child = Command::new(BINARY)
        .env("MIKE_INSPECT_URL", "http://127.0.0.1:1")
        .env("MIKE_MCP_PROFILE", "full")
        .env("RUST_LOG", "info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stderr"))?;
    let mut reader = BufReader::new(stdout);

    stdin
        .write_all(format!("{}\n", initialize_request()).as_bytes())
        .await?;
    stdin
        .write_all(format!("{}\n", initialized_notification()).as_bytes())
        .await?;

    let init_response = read_next_message(&mut reader).await?;
    assert_eq!(init_response["jsonrpc"], "2.0");
    assert_eq!(init_response["id"], 1);
    let result = &init_response["result"];
    assert!(
        result["serverInfo"]["name"] == "mike-mcp",
        "unexpected serverInfo: {result}"
    );
    assert!(
        result["capabilities"]["tools"].is_object(),
        "server must advertise tools capability: {result}"
    );
    assert!(
        result["capabilities"]["resources"].is_object(),
        "server must advertise resources capability: {result}"
    );

    stdin
        .write_all(format!("{}\n", list_tools_request(2)).as_bytes())
        .await?;
    let tools_response = read_next_message(&mut reader).await?;
    assert_eq!(tools_response["id"], 2);
    let tools = tools_response["result"]["tools"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tools array missing"))?;
    let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_full_catalogue_contains_expected_tools(&tool_names);
    assert_eq!(
        tools.len(),
        mike_mcp::tools::all_tools().len(),
        "tools/list must serve every tool in all_tools() ({} expected), got {} ({tool_names:?})",
        mike_mcp::tools::all_tools().len(),
        tools.len()
    );

    drop(stdin);
    let _ = child.wait().await;

    assert_stderr_shows_tracing_was_initialized(stderr).await;

    Ok(())
}

fn assert_full_catalogue_contains_expected_tools(tool_names: &[&str]) {
    for expected in [
        "list_requests",
        "get_request",
        "get_request_summary",
        "get_request_body",
        "get_request_sent_body",
        "get_response_body",
        "list_sessions",
        "get_session",
        "diagnostic_stats",
        "diagnostic_findings",
        "get_stats",
        "list_filter_rules",
        "create_filter_rule",
        "update_filter_rule",
        "delete_filter_rule",
        "list_prompt_compositions",
        "get_prompt_composition",
        "create_prompt_composition",
        "update_prompt_composition",
        "delete_prompt_composition",
        "list_prompt_anomalies",
        "list_session_tool_uses",
        "list_tool_errors",
        "list_operator_turns",
        "get_operator_turn",
        "get_session_operator_turns",
        "get_session_overview",
        "list_corrected_turns",
        "list_outcome_summary",
        "get_accumulated_text",
        "get_request_stream_chunks",
        "dry_run_filter_rules",
        "list_projects",
        "get_session_head",
        "find_text",
    ] {
        assert!(
            tool_names.contains(&expected),
            "tools/list is missing {expected}. Got: {tool_names:?}"
        );
    }
}

async fn assert_stderr_shows_tracing_was_initialized(stderr: tokio::process::ChildStderr) {
    let mut stderr_reader = BufReader::new(stderr);
    let mut stderr_line = String::new();
    let _ = timeout(
        Duration::from_secs(2),
        stderr_reader.read_line(&mut stderr_line),
    )
    .await;
    assert!(
        !stderr_line.is_empty(),
        "stderr should contain tracing output, but was empty"
    );
}

#[tokio::test]
async fn default_profile_serves_lean_core_subset() -> anyhow::Result<()> {
    use std::collections::HashSet;

    let mut child = Command::new(BINARY)
        .env("MIKE_INSPECT_URL", "http://127.0.0.1:1")
        .env_remove("MIKE_MCP_PROFILE")
        .env("RUST_LOG", "info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stdout"))?;
    let mut reader = BufReader::new(stdout);

    stdin
        .write_all(format!("{}\n", initialize_request()).as_bytes())
        .await?;
    stdin
        .write_all(format!("{}\n", initialized_notification()).as_bytes())
        .await?;
    let _init = read_next_message(&mut reader).await?;

    stdin
        .write_all(format!("{}\n", list_tools_request(2)).as_bytes())
        .await?;
    let tools_response = read_next_message(&mut reader).await?;
    let tools = tools_response["result"]["tools"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tools array missing"))?;
    let served: HashSet<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    let full_only = mike_mcp::profiles::FULL_ONLY_TOOLS;
    let expected: HashSet<&str> = mike_mcp::tools::CORE_TOOL_NAMES
        .iter()
        .chain(mike_mcp::tools::mail::MAIL_TOOL_NAMES.iter())
        .copied()
        .filter(|name| !full_only.contains(name))
        .collect();
    assert_eq!(
        served, expected,
        "default profile must serve CORE_TOOL_NAMES + the mail domain without the full-only tools; got {served:?}"
    );

    for mail_tool in mike_mcp::tools::mail::MAIL_TOOL_NAMES
        .iter()
        .filter(|name| !full_only.contains(name))
    {
        assert!(
            served.contains(mail_tool),
            "{mail_tool} must be served by the default profile: a mailed session could not act on it"
        );
    }
    for tool in full_only {
        assert!(!served.contains(tool), "{tool} is served only under full");
    }

    assert!(
        served.len() < mike_mcp::tools::all_tools().len(),
        "core ({}) must be smaller than full ({})",
        served.len(),
        mike_mcp::tools::all_tools().len()
    );

    for deferred in [
        "diagnostic_memory",
        "create_prompt_composition",
        "list_prompt_anomalies",
    ] {
        assert!(
            !served.contains(deferred),
            "{deferred} must be Full-only but was served by the default core profile"
        );
    }

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}

#[tokio::test]
async fn call_tool_against_unreachable_inspect_returns_json_rpc_error() -> anyhow::Result<()> {
    let mut child = Command::new(BINARY)
        .env("MIKE_INSPECT_URL", "http://127.0.0.1:1")
        .env("RUST_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open stdout"))?;
    let mut reader = BufReader::new(stdout);

    stdin
        .write_all(format!("{}\n", initialize_request()).as_bytes())
        .await?;
    stdin
        .write_all(format!("{}\n", initialized_notification()).as_bytes())
        .await?;
    let _init = read_next_message(&mut reader).await?;

    stdin
        .write_all(format!("{}\n", call_list_requests_tool(3)).as_bytes())
        .await?;
    let response = read_next_message(&mut reader).await?;
    assert_eq!(response["id"], 3);

    let is_rpc_error = response["error"].is_object();
    let is_tool_error = response["result"]["isError"] == Value::Bool(true);
    assert!(
        is_rpc_error || is_tool_error,
        "expected either JSON-RPC error or CallToolResult.isError=true, \
         got: {response}"
    );

    let text = serde_json::to_string(&response)?;
    assert!(
        text.contains("127.0.0.1:1") || text.contains("unreachable"),
        "error response should mention the unreachable inspect URL: {response}"
    );

    drop(stdin);
    let _ = child.wait().await;
    Ok(())
}
