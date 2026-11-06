use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::*;

const SUCCESS: &str = r#"{"duration_api_ms":1231,"stop_reason":"tool_use","session_id":"706abdc8-9caf-4020-bedf-da5a7f824d54","total_cost_usd":0.00026,"usage":{"input_tokens":2,"cache_creation_input_tokens":1128,"cache_read_input_tokens":40,"output_tokens":86,"output_tokens_details":{"thinking_tokens":5}},"modelUsage":{"claude-haiku-5-5":{"inputTokens":2}},"is_error":false,"subtype":"success","result":"{\"answer\":\"hi\",\"n\":3}","structured_output":{"answer":"hi","n":3},"type":"result"}"#;

const LOGGED_OUT: &str = r#"{"is_error":true,"subtype":"success","api_error_status":null,"result":"Not logged in · Please run /login","type":"result"}"#;

const BAD_MODEL: &str = r#"{"is_error":true,"subtype":"success","api_error_status":404,"result":"There's an issue with the selected model (nope). It may not exist or you may not have access to it.","type":"result"}"#;

fn schema() -> Value {
    json!({"type": "object", "properties": {"n": {"type": "integer"}}})
}

fn request<'a>(schema: &'a Value, effort: &'a str, timeout: Duration) -> ClaudeRequest<'a> {
    ClaudeRequest {
        model: "haiku",
        system: "answer in json",
        user: "say hi",
        schema,
        reasoning_effort: effort,
        timeout,
    }
}

fn fake_claude(dir: &Path, body: &str) -> anyhow::Result<PathBuf> {
    let path = dir.join("claude");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

#[test]
fn claude_effort_passes_real_levels_and_drops_none() {
    assert_eq!(claude_effort("low"), Some("low"));
    assert_eq!(claude_effort("high"), Some("high"));
    assert_eq!(claude_effort("none"), None);
    assert_eq!(claude_effort(""), None);
    assert_eq!(claude_effort("minimal"), None);
}

#[test]
fn cli_args_run_print_mode_without_tools_settings_or_session() {
    let args = cli_args(
        "haiku",
        Some("low"),
        r#"{"type":"object"}"#,
        Path::new("/tmp/w/s.txt"),
    );
    assert_eq!(args[0], "-p");
    for flag in [
        "--no-session-persistence",
        "--disable-slash-commands",
        "--strict-mcp-config",
    ] {
        assert!(args.iter().any(|a| a == flag), "missing {flag}");
    }
    let joined = args.join("\u{1f}");
    assert!(joined.contains("--output-format\u{1f}json"));
    assert!(joined.contains("--setting-sources\u{1f}\u{1f}--disable-slash-commands"));
    assert!(joined.contains("--tools\u{1f}\u{1f}--model\u{1f}haiku"));
    assert!(joined.contains("--effort\u{1f}low"));
    assert!(joined.contains("--json-schema\u{1f}{\"type\":\"object\"}"));
    assert!(joined.ends_with("--system-prompt-file\u{1f}/tmp/w/s.txt"));
}

#[test]
fn cli_args_without_effort_omit_the_flag() {
    let args = cli_args("opus", None, "{}", Path::new("/tmp/s.txt"));
    assert!(!args.iter().any(|a| a == "--effort"));
}

#[test]
fn completion_payload_carries_structured_output_usage_and_session_id() -> anyhow::Result<()> {
    let result = result_object(SUCCESS).ok_or_else(|| anyhow::anyhow!("no result"))?;
    let payload = completion_payload("haiku", &result).map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(
        payload.pointer("/id").and_then(Value::as_str),
        Some("706abdc8-9caf-4020-bedf-da5a7f824d54")
    );
    assert_eq!(
        payload.pointer("/model").and_then(Value::as_str),
        Some("claude-haiku-5-5")
    );
    let content = payload
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("no content"))?;
    assert_eq!(
        serde_json::from_str::<Value>(content)?,
        json!({"answer": "hi", "n": 3})
    );
    assert_eq!(
        payload
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_i64),
        Some(1170)
    );
    assert_eq!(
        payload
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_i64),
        Some(86)
    );
    assert_eq!(
        payload
            .pointer("/usage/prompt_tokens_details/cached_tokens")
            .and_then(Value::as_i64),
        Some(40)
    );
    assert_eq!(
        payload
            .pointer("/usage/completion_tokens_details/reasoning_tokens")
            .and_then(Value::as_i64),
        Some(5)
    );
    assert!(payload.pointer("/usage/cost").is_some_and(Value::is_null));
    Ok(())
}

#[test]
fn completion_payload_without_structured_output_uses_the_result_text() -> anyhow::Result<()> {
    let result = json!({"is_error": false, "subtype": "success", "result": "{\"n\":1}"});
    let payload = completion_payload("haiku", &result).map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(
        payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str),
        Some("{\"n\":1}")
    );
    assert_eq!(
        payload.pointer("/model").and_then(Value::as_str),
        Some("haiku")
    );
    Ok(())
}

#[test]
fn completion_payload_error_result_is_an_error_with_the_cli_message() {
    let result = result_object(LOGGED_OUT).unwrap_or(Value::Null);
    let err = completion_payload("haiku", &result)
        .err()
        .unwrap_or_default();
    assert_eq!(err, "Not logged in · Please run /login");
}

#[test]
fn completion_payload_names_the_api_status_when_there_is_one() {
    let result = result_object(BAD_MODEL).unwrap_or(Value::Null);
    let err = completion_payload("nope", &result)
        .err()
        .unwrap_or_default();
    assert!(err.contains("selected model (nope)"), "{err}");
    assert!(err.ends_with("(API status 404)"), "{err}");
}

#[test]
fn completion_payload_non_success_subtype_is_an_error() {
    let result = json!({"is_error": false, "subtype": "error_max_turns"});
    let err = completion_payload("haiku", &result)
        .err()
        .unwrap_or_default();
    assert!(err.contains("error_max_turns"), "{err}");
}

#[test]
fn completion_payload_with_no_content_is_an_error() {
    let result = json!({"is_error": false, "subtype": "success"});
    assert!(completion_payload("haiku", &result).is_err());
}

#[test]
fn result_object_reads_an_array_of_messages_and_rejects_garbage() {
    let array = format!("[{{\"type\":\"system\"}},{SUCCESS}]");
    assert!(result_object(&array).is_some_and(|r| r.get("subtype").is_some()));
    assert!(result_object("not json").is_none());
    assert!(result_object("42").is_none());
}

#[test]
fn failure_message_prefers_the_result_error_over_stderr() {
    assert_eq!(
        failure_message(LOGGED_OUT, "ignored stderr"),
        "Not logged in · Please run /login"
    );
}

#[test]
fn failure_message_without_a_result_falls_back_to_stderr_then_stdout() {
    assert_eq!(failure_message("not json", "  boom \n"), "boom");
    assert_eq!(failure_message(" plain ", ""), "plain");
}

#[test]
fn effort_rejected_matches_the_word_effort() {
    assert!(effort_rejected(
        "This model does not support the effort parameter"
    ));
    assert!(!effort_rejected("Not logged in"));
}

#[tokio::test]
async fn run_with_missing_binary_is_a_transport_error() {
    let schema = schema();
    let outcome = run(
        "/nonexistent/mike-claude-binary",
        request(&schema, "low", Duration::from_secs(5)),
    )
    .await;
    assert!(
        matches!(outcome, Err(LlmError::Transport(ref m)) if m.contains("could not start")),
        "a missing binary must be a transport error, got {outcome:?}"
    );
}

#[tokio::test]
async fn run_reads_the_prompt_from_stdin_and_returns_the_payload() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let bin = fake_claude(
        dir.path(),
        &format!(
            "input=$(cat)\ncase \"$input\" in *'say hi'*) ;; *) exit 9;; esac\ncat <<'EOF'\n{SUCCESS}\nEOF"
        ),
    )?;
    let schema = schema();
    let payload = run(
        &bin.display().to_string(),
        request(&schema, "none", Duration::from_secs(10)),
    )
    .await?;
    assert_eq!(
        payload
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_i64),
        Some(86)
    );
    Ok(())
}

#[tokio::test]
async fn run_logged_out_cli_is_a_transport_error_naming_the_login() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let bin = fake_claude(
        dir.path(),
        &format!("cat <<'EOF'\n{LOGGED_OUT}\nEOF\nexit 1"),
    )?;
    let schema = schema();
    let outcome = run(
        &bin.display().to_string(),
        request(&schema, "none", Duration::from_secs(10)),
    )
    .await;
    match outcome {
        Err(LlmError::Transport(message)) => {
            assert!(message.contains("Not logged in"), "{message}");
            Ok(())
        }
        other => anyhow::bail!("expected a transport error, got {other:?}"),
    }
}

#[tokio::test]
async fn run_effort_rejection_is_retried_once_without_the_flag() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let rejected = r#"{"is_error":true,"subtype":"success","result":"model does not support effort","type":"result"}"#;
    let body = format!(
        "case \"$*\" in *--effort*) printf '%s' '{rejected}'; exit 1;; esac\ncat <<'EOF'\n{SUCCESS}\nEOF"
    );
    let bin = fake_claude(dir.path(), &body)?;
    let schema = schema();
    let payload = run(
        &bin.display().to_string(),
        request(&schema, "high", Duration::from_secs(10)),
    )
    .await?;
    assert!(payload.pointer("/choices/0/message/content").is_some());
    Ok(())
}

#[tokio::test]
async fn run_exceeding_the_timeout_is_a_timeout_error() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let bin = fake_claude(dir.path(), "sleep 30")?;
    let schema = schema();
    let outcome = run(
        &bin.display().to_string(),
        request(&schema, "none", Duration::from_millis(200)),
    )
    .await;
    assert!(matches!(outcome, Err(LlmError::Timeout)), "{outcome:?}");
    Ok(())
}

struct EnvVarGuard {
    key: &'static str,
    prior: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prior = std::env::var_os(key);
        unsafe { std::env::set_var(key, value) };
        Self { key, prior }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

#[tokio::test]
#[serial_test::serial(claude_env)]
async fn run_scrubs_the_environment_the_cli_sees() -> anyhow::Result<()> {
    let _base = EnvVarGuard::set("ANTHROPIC_BASE_URL", "http://127.0.0.1:9988");
    let _proxy = EnvVarGuard::set("HTTPS_PROXY", "http://127.0.0.1:9987");
    let dir = tempfile::tempdir()?;
    let bin = fake_claude(
        dir.path(),
        &format!(
            "if [ -n \"$ANTHROPIC_BASE_URL\" ] || [ -n \"$HTTPS_PROXY\" ]; then exit 7; fi\ncat <<'EOF'\n{SUCCESS}\nEOF"
        ),
    )?;
    let schema = schema();
    let payload = run(
        &bin.display().to_string(),
        request(&schema, "none", Duration::from_secs(10)),
    )
    .await;
    assert!(payload.is_ok(), "{payload:?}");
    Ok(())
}

#[tokio::test]
async fn run_missing_binary_names_the_resolved_path() {
    let schema = schema();
    let outcome = run(
        "/nonexistent/mike-claude-binary",
        request(&schema, "low", Duration::from_secs(5)),
    )
    .await;
    assert!(
        matches!(outcome, Err(LlmError::Transport(ref m)) if m.contains("resolved path /nonexistent/mike-claude-binary")),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn run_silent_cli_reports_its_path_and_that_it_produced_no_output() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let bin = fake_claude(dir.path(), "cat >/dev/null\nexit 0")?;
    let schema = schema();
    let outcome = run(
        &bin.display().to_string(),
        request(&schema, "none", Duration::from_secs(10)),
    )
    .await;
    let Err(LlmError::Transport(message)) = outcome else {
        anyhow::bail!("a silent claude must be a transport error, got {outcome:?}");
    };
    assert!(message.contains(&bin.display().to_string()), "{message}");
    assert!(message.contains("produced no output"), "{message}");
    Ok(())
}
