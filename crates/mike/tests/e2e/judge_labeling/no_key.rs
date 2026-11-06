use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::db_helpers;

use wiremock::MockServer;

use super::support::{count_judged_turns, seed_unjudged_turns, start_proxy_without_openrouter_key};

const KEY_ENV_VARS: [&str; 2] = ["MIKE_STEERER_OPENROUTER_API_KEY", "OPENROUTER_API_KEY"];
const CLAUDE_BIN_ENV: &str = "MIKE_CLAUDE_CLI_BIN";

const CLI_VERDICT: &str = r#"{"is_error":false,"subtype":"success","session_id":"sess-keyless","usage":{"input_tokens":3,"cache_creation_input_tokens":100,"cache_read_input_tokens":20,"output_tokens":9},"modelUsage":{"claude-haiku-5-5":{}},"structured_output":{"label":"accepted","confidence":0.9,"rationale":"delivered what the operator asked for","unmet_ask":"","evidence_quality":"verified"},"type":"result"}"#;

struct KeylessEnv {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl KeylessEnv {
    fn with_claude_bin(bin: &Path) -> Self {
        let mut saved = Vec::new();
        for name in KEY_ENV_VARS {
            saved.push((name, std::env::var_os(name)));
            unsafe { std::env::remove_var(name) };
        }
        saved.push((CLAUDE_BIN_ENV, std::env::var_os(CLAUDE_BIN_ENV)));
        unsafe { std::env::set_var(CLAUDE_BIN_ENV, bin) };
        Self { saved }
    }
}

impl Drop for KeylessEnv {
    fn drop(&mut self) {
        for (name, prior) in &self.saved {
            match prior {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

fn fake_claude(dir: &Path) -> anyhow::Result<std::path::PathBuf> {
    let path = dir.join("claude");
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncat >/dev/null\ncat <<'EOF'\n{CLI_VERDICT}\nEOF\n"),
    )?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

#[tokio::test]
#[serial_test::serial(openrouter_env)]
async fn judge_sweep_runs_on_a_claude_model_with_no_openrouter_key_and_records_the_claude_transport()
-> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping the keyless judge sweep");
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let bin_dir = tempfile::tempdir()?;
    let _env = KeylessEnv::with_claude_bin(&fake_claude(bin_dir.path())?);

    let session_id = "52345678-f001-4f01-8f01-f01f01f01f05";
    seed_unjudged_turns(&pool, "test-judge-keyless", session_id, 3).await?;

    let (shutdown, state) = start_proxy_without_openrouter_key(
        &upstream_mock.uri(),
        &test_db.database_url,
        &[("judge_worker.model", serde_json::json!("claude/haiku"))],
    )
    .await?;

    mike::llm::reset_global_for_tests();
    let labeled = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        labeled, 3,
        "a claude/ judge model must label every turn with no OpenRouter key set, got {labeled}"
    );
    assert_eq!(count_judged_turns(&pool, session_id).await?, 3);

    let mut transports: Vec<(String, String)> = Vec::new();
    for _ in 0..100 {
        transports = sqlx::query_as(
            "SELECT model, request_body->>'transport'
               FROM llm.calls
              WHERE role = 'judge_worker' AND outcome = 'ok'",
        )
        .fetch_all(&pool)
        .await?;
        if transports.len() >= 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(transports.len(), 3, "{transports:?}");
    for (model, transport) in &transports {
        assert_eq!(model, "claude/haiku");
        assert_eq!(transport, "claude");
    }

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
