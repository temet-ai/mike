use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::test_support::{TestSink, TestVerdict, test_schema};
use super::*;
use crate::llm::provider::tests::InstalledProvider;

const CLI_SUCCESS: &str = r#"{"is_error":false,"subtype":"success","session_id":"sess-1","usage":{"input_tokens":3,"cache_creation_input_tokens":100,"cache_read_input_tokens":20,"output_tokens":9},"modelUsage":{"claude-sonnet-5-5":{}},"structured_output":{"label":true},"result":"{\"label\":true}","type":"result"}"#;

const CLI_LOGGED_OUT: &str = r#"{"is_error":true,"subtype":"success","result":"Not logged in · Please run /login","type":"result"}"#;

struct BinEnv {
    prior: Option<std::ffi::OsString>,
}

impl BinEnv {
    fn point_at(bin: &std::path::Path) -> Self {
        let prior = std::env::var_os(crate::llm::claude::BIN_ENV);
        unsafe { std::env::set_var(crate::llm::claude::BIN_ENV, bin) };
        Self { prior }
    }
}

impl Drop for BinEnv {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => unsafe { std::env::set_var(crate::llm::claude::BIN_ENV, v) },
            None => unsafe { std::env::remove_var(crate::llm::claude::BIN_ENV) },
        }
    }
}

fn fake_cli(dir: &std::path::Path, body: &str) -> anyhow::Result<PathBuf> {
    let path = dir.join("claude");
    std::fs::write(&path, format!("#!/bin/sh\ncat >/dev/null\n{body}\n"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn client(model: &str, sink: Arc<TestSink>, role: LlmRole) -> anyhow::Result<OpenRouterClient> {
    let limiter = CallLimiter::new(1000, Duration::from_secs(60), 1000, Duration::from_secs(60));
    Ok(OpenRouterClient::new_with_limiter(
        String::new(),
        model.to_owned(),
        None,
        None,
        Arc::new(limiter),
    )?
    .with_usage_sink(sink)
    .with_role(role))
}

#[test]
#[serial_test::serial(llm_provider)]
fn routed_model_follows_the_role_tier_under_a_provider() -> anyhow::Result<()> {
    let _installed = InstalledProvider::with("claude");
    let sink = Arc::new(TestSink::default());
    let steerer = client("google/own", sink.clone(), LlmRole::Steerer)?;
    assert_eq!(steerer.provider_routed().model(), "claude/opus");
    let adjudicator = steerer.with_tier(ModelTier::Fast);
    assert_eq!(adjudicator.provider_routed().model(), "claude/haiku");
    let judge = client("google/own", sink.clone(), LlmRole::JudgeWorker)?;
    assert_eq!(judge.provider_routed().model(), "claude/sonnet");
    Ok(())
}

#[test]
#[serial_test::serial(llm_provider)]
fn routed_model_leaves_evaluation_and_unattributed_clients_alone() -> anyhow::Result<()> {
    let _installed = InstalledProvider::with("codex");
    let sink = Arc::new(TestSink::default());
    let eval = client("google/own", sink.clone(), LlmRole::Eval)?;
    assert_eq!(eval.provider_routed().model(), "google/own");
    let loose = client("google/own", sink, LlmRole::Unattributed)?;
    assert_eq!(loose.provider_routed().model(), "google/own");
    Ok(())
}

#[test]
#[serial_test::serial(llm_provider)]
fn routed_model_is_the_roles_own_model_under_per_role() -> anyhow::Result<()> {
    let sink = Arc::new(TestSink::default());
    let steerer = client("google/own", sink, LlmRole::Steerer)?;
    assert_eq!(steerer.provider_routed().model(), "google/own");
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn verdict_under_the_claude_provider_runs_the_cli_and_records_a_subscription_call()
-> anyhow::Result<()> {
    let _installed = InstalledProvider::with("claude");
    let dir = tempfile::tempdir()?;
    let _bin = BinEnv::point_at(&fake_cli(
        dir.path(),
        &format!("cat <<'EOF'\n{CLI_SUCCESS}\nEOF"),
    )?);
    let sink = Arc::new(TestSink::default());
    let judge = client("google/own", sink.clone(), LlmRole::JudgeWorker)?;

    let verdict: TestVerdict = judge.verdict("sys", "user", test_schema()).await?;

    assert_eq!(verdict, TestVerdict { label: true });
    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.model, "claude/sonnet");
    assert_eq!(call.role, LlmRole::JudgeWorker);
    assert!(matches!(call.outcome, LlmCallOutcome::Ok));
    assert_eq!(call.cost_usd, None);
    assert_eq!(call.prompt_tokens, Some(123));
    assert_eq!(call.completion_tokens, Some(9));
    assert_eq!(call.cached_tokens, Some(20));
    assert_eq!(call.generation_id.as_deref(), Some("sess-1"));
    let body = call
        .request_body
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("a claude call must record its request body"))?;
    assert_eq!(body["transport"], "claude");
    assert_eq!(body["model"], "claude/sonnet");
    assert_eq!(body["messages"][0]["content"], "sys");
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn verdict_under_the_claude_provider_records_a_transport_error_when_logged_out()
-> anyhow::Result<()> {
    let _installed = InstalledProvider::with("claude");
    let dir = tempfile::tempdir()?;
    let _bin = BinEnv::point_at(&fake_cli(
        dir.path(),
        &format!("cat <<'EOF'\n{CLI_LOGGED_OUT}\nEOF\nexit 1"),
    )?);
    let sink = Arc::new(TestSink::default());
    let steerer = client("google/own", sink.clone(), LlmRole::Steerer)?;

    let outcome = steerer
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(
        matches!(outcome, Err(LlmError::Transport(ref m)) if m.contains("Not logged in")),
        "{outcome:?}"
    );
    let calls = sink.recorded();
    assert!(!calls.is_empty());
    for call in &calls {
        assert!(matches!(call.outcome, LlmCallOutcome::TransportError));
        assert_eq!(call.model, "claude/opus");
        assert_eq!(
            call.request_body.as_ref().map(|b| b["transport"].clone()),
            Some(serde_json::json!("claude"))
        );
    }
    Ok(())
}

#[tokio::test]
#[serial_test::serial(llm_provider)]
async fn a_claude_model_set_on_one_role_runs_the_cli_without_any_provider() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let _bin = BinEnv::point_at(&fake_cli(
        dir.path(),
        &format!("cat <<'EOF'\n{CLI_SUCCESS}\nEOF"),
    )?);
    let sink = Arc::new(TestSink::default());
    let judge = client("claude/haiku", sink.clone(), LlmRole::SessionJudge)?;

    let verdict: TestVerdict = judge.verdict("sys", "user", test_schema()).await?;

    assert_eq!(verdict, TestVerdict { label: true });
    assert_eq!(
        sink.recorded().first().map(|c| c.model.clone()).as_deref(),
        Some("claude/haiku")
    );
    Ok(())
}
