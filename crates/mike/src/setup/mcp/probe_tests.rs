use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::super::{ProbeReport, admit, probe, register};

const INSTANCE: &str = "test-instance";
const INSPECT_URL: &str = "http://127.0.0.1:14589";

fn initialize_reply(version: &str) -> String {
    json!({"jsonrpc":"2.0","id":1,"result":{
        "protocolVersion":"2024-11-05","capabilities":{"tools":{}},
        "serverInfo":{"name":"mike-mcp","version":version}
    }})
    .to_string()
}

fn tools_reply() -> String {
    json!({"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"get_stats"}]}}).to_string()
}

fn stats_reply(instance: &Value) -> String {
    let body = json!({"stats":{"runtime":{},"diagnostic":{},"health":{"instance":instance}}});
    json!({"jsonrpc":"2.0","id":3,"result":{
        "content":[{"type":"text","text":body.to_string()}],"isError":false
    }})
    .to_string()
}

fn this_instance() -> Value {
    json!({"instance_id": INSTANCE, "boot_generation": uuid::Uuid::nil()})
}

const ANSWERED: Duration = Duration::from_secs(60);
const SILENT: Duration = Duration::from_millis(50);

async fn converse(answer: impl Fn(u64) -> Option<String> + Send + 'static) -> &'static str {
    converse_within(ANSWERED, answer).await
}

async fn converse_within(
    step: Duration,
    answer: impl Fn(u64) -> Option<String> + Send + 'static,
) -> &'static str {
    let (client_out, server_in) = tokio::io::duplex(1 << 16);
    let (mut server_out, client_in) = tokio::io::duplex(1 << 16);
    tokio::spawn(async move {
        let mut requests = BufReader::new(server_in).lines();
        while let Ok(Some(line)) = requests.next_line().await {
            let request: Value = serde_json::from_str(&line).unwrap_or_default();
            let Some(id) = request["id"].as_u64() else {
                continue;
            };
            match answer(id) {
                Some(reply) => {
                    if server_out
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                None => std::future::pending::<()>().await,
            }
        }
    });
    let (_, _, code, _) =
        probe::converse(client_out, BufReader::new(client_in), INSTANCE, step).await;
    code
}

fn handshake_then(
    read: impl Fn() -> Option<String> + Send + 'static,
) -> impl Fn(u64) -> Option<String> + Send + 'static {
    move |id| match id {
        1 => Some(initialize_reply(env!("CARGO_PKG_VERSION"))),
        2 => Some(tools_reply()),
        _ => read(),
    }
}

#[tokio::test]
async fn read_from_the_selected_instance_is_ready() {
    let code = converse(handshake_then(|| Some(stats_reply(&this_instance())))).await;
    assert_eq!(code, "MCP_READY");
}

#[tokio::test]
async fn backend_errors_are_unavailable_not_a_handshake_failure() {
    let rpc_error =
        r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32603,"message":"backend offline"}}"#;
    let tool_error = r#"{"jsonrpc":"2.0","id":3,"result":{"content":[],"isError":true}}"#;
    for reply in [rpc_error, tool_error] {
        let code = converse(handshake_then(move || Some(reply.to_owned()))).await;
        assert_eq!(code, "MCP_BACKEND_UNAVAILABLE", "{reply}");
    }
}

#[tokio::test]
async fn non_protocol_output_on_read_is_a_protocol_failure() {
    let code = converse(handshake_then(|| Some("diagnostic on stdout".to_owned()))).await;
    assert_eq!(code, "MCP_READ_PROTOCOL_FAILED");
}

#[tokio::test]
async fn stats_from_another_or_unidentified_instance_are_a_mismatch() {
    for instance in [
        json!({"instance_id": "other-config", "boot_generation": uuid::Uuid::nil()}),
        json!({"instance_id": INSTANCE}),
        json!({"instance_id": INSTANCE, "boot_generation": "not-a-uuid"}),
    ] {
        let reply = stats_reply(&instance);
        let code = converse(handshake_then(move || Some(reply.clone()))).await;
        assert_eq!(code, "MCP_INSTANCE_MISMATCH", "{instance}");
    }
}

#[tokio::test]
async fn wrong_server_version_or_silent_initialize_is_a_handshake_failure() {
    let code = converse(|id| (id == 1).then(|| initialize_reply("0.0.0-unrelated"))).await;
    assert_eq!(code, "MCP_HANDSHAKE_FAILED");
    let code = converse_within(SILENT, |_| None).await;
    assert_eq!(code, "MCP_HANDSHAKE_FAILED");
}

#[tokio::test]
async fn notifications_before_a_response_are_skipped() {
    let code = converse(move |id| match id {
        1 => Some(format!(
            "{}\n{}",
            r#"{"jsonrpc":"2.0","method":"notifications/message","params":{}}"#,
            initialize_reply(env!("CARGO_PKG_VERSION"))
        )),
        2 => Some(tools_reply()),
        _ => Some(stats_reply(&this_instance())),
    })
    .await;
    assert_eq!(code, "MCP_READY");
}

#[test]
fn companion_must_report_exactly_this_mike_version() -> Result<()> {
    probe::check_version_output(format!("mike-mcp {}\n", env!("CARGO_PKG_VERSION")).as_bytes())?;
    for output in [
        b"mike-mcp 0.0.0-unrelated\n".to_vec(),
        b"something else".to_vec(),
        vec![b'x'; 5000],
    ] {
        assert!(probe::check_version_output(&output).is_err());
    }
    Ok(())
}

fn report(handshake: bool, backend_read: bool, code: &'static str) -> ProbeReport {
    ProbeReport {
        binary: PathBuf::from("/fixture/mike-mcp"),
        inspect_url: INSPECT_URL.into(),
        instance_id: INSTANCE.into(),
        version: env!("CARGO_PKG_VERSION"),
        handshake,
        backend_read,
        code,
        detail: "fixture",
    }
}

#[test]
fn registration_admits_a_ready_or_offline_backend_and_nothing_else() {
    assert!(admit(&report(true, true, "MCP_READY")).is_ok());
    assert!(admit(&report(true, false, "MCP_BACKEND_UNAVAILABLE")).is_ok());
    for refused in [
        report(true, false, "MCP_INSTANCE_MISMATCH"),
        report(true, false, "MCP_READ_PROTOCOL_FAILED"),
        report(false, false, "MCP_HANDSHAKE_FAILED"),
    ] {
        assert!(admit(&refused).is_err(), "{}", refused.code);
    }
}

#[tokio::test]
async fn registration_with_an_unusable_companion_never_changes_the_client() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut plan = super::plan(dir.path());
    std::fs::write(&plan.client_config, "{}")?;
    plan.binary = dir.path().join("missing/mike-mcp");
    assert!(register(plan.clone(), false).await.is_err());
    assert_eq!(std::fs::read_to_string(&plan.client_config)?, "{}");
    Ok(())
}

fn companion_script(directory: &Path) -> Result<PathBuf> {
    let path = directory.join("mcp fixture");
    let script = format!(
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then printf '%s\n' 'mike-mcp {version}'; exit 0; fi
[ "$1" = "--core" ] || exit 8
[ "$MIKE_INSPECT_URL" = '{url}' ] || exit 9
[ -z "$CLAUDE_CODE_SESSION_ID$MIKE_SESSION_ID" ] || exit 10
printf '%s\n' "$$" > '{pid}'
IFS= read -r initialize
printf '%s\n' '{initialize}'
IFS= read -r initialized
IFS= read -r list
printf '%s\n' '{tools}'
IFS= read -r read
printf '%s\n' '{stats}'
while IFS= read -r remaining; do :; done
"#,
        version = env!("CARGO_PKG_VERSION"),
        url = INSPECT_URL,
        pid = directory.join("pid").display(),
        initialize = initialize_reply(env!("CARGO_PKG_VERSION")),
        tools = tools_reply(),
        stats = stats_reply(&this_instance()),
    );
    std::fs::write(&path, script)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

#[tokio::test]
async fn real_companion_process_is_verified_over_stdio_and_reaped() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let binary = companion_script(dir.path())?;
    let report = probe::run(&binary, INSPECT_URL, INSTANCE).await?;
    assert_eq!(report.code, "MCP_READY");
    let pid: libc::pid_t = std::fs::read_to_string(dir.path().join("pid"))?
        .trim()
        .parse()?;
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    Ok(())
}
