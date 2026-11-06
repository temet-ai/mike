use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

const BOOT_LINE_DEADLINE: Duration = Duration::from_secs(180);

fn mike_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mike")
}

async fn spawn_and_poll_for(
    nested_cwd: &Path,
    proxy_addr: &str,
    inspect_addr: &str,
    extra_envs: &[(&str, &str)],
    needle: &str,
    deadline: Duration,
) -> anyhow::Result<String> {
    let mut cmd = Command::new(mike_bin());
    cmd.env_clear()
        .env("HOME", nested_cwd)
        .env("XDG_CONFIG_HOME", nested_cwd.join("user-config"))
        .env("XDG_STATE_HOME", nested_cwd.join("user-state"))
        .arg("serve")
        .current_dir(nested_cwd)
        .env("MIKE_LISTEN_ADDR", proxy_addr)
        .env("MIKE_INSPECT_ADDR", inspect_addr)
        .env("MIKE_SPA_ENABLED", "0")
        .env("MIKE_PERSIST_ENABLED", "0")
        .env("MIKE_SMOKE_RUN", "1")
        .env("RUST_LOG", "info")
        .env_remove("MIKE_DATABASE_URL")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (key, value) in extra_envs {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("child stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("child stderr was not piped"))?;

    let captured = Arc::new(Mutex::new(String::new()));
    let drains = [
        tokio::spawn(drain_lines(stdout, captured.clone())),
        tokio::spawn(drain_lines(stderr, captured.clone())),
    ];

    let start = std::time::Instant::now();
    let mut exited = false;
    loop {
        let found = captured.lock().await.contains(needle);
        if found || start.elapsed() >= deadline {
            break;
        }
        if child.try_wait()?.is_some() {
            exited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if exited {
        for drain in drains {
            let _ = drain.await;
        }
    } else if let Some(pid) = child.id() {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = child.wait().await;

    let result = captured.lock().await.clone();
    Ok(result)
}

async fn drain_lines(reader: impl tokio::io::AsyncRead + Unpin, buf: Arc<Mutex<String>>) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut guard = buf.lock().await;
        guard.push_str(&line);
        guard.push('\n');
    }
}

#[tokio::test]
async fn explicit_db_url_wins_over_unread_ancestor_dotenv_on_real_boot() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    std::fs::write(
        tmp.path().join(".env"),
        "MIKE_DATABASE_URL=postgres://dotenv-should-not-win@127.0.0.1:6666/claude_proxy\n",
    )?;
    let nested = tmp.path().join("worktrees/nested/subdir");
    std::fs::create_dir_all(&nested)?;

    let combined = spawn_and_poll_for(
        &nested,
        "127.0.0.1:19994",
        "127.0.0.1:19995",
        &[(
            "MIKE_DATABASE_URL",
            "postgres://explicit-scratch@127.0.0.1:6666/scratch_db",
        )],
        "resolved capture database target",
        BOOT_LINE_DEADLINE,
    )
    .await?;

    assert!(
        !combined.contains("dotenv-should-not-win"),
        "an unselected ancestor dotenv must never reach boot; captured: {combined}"
    );
    assert!(
        combined.contains("resolved capture database target")
            && combined.contains("explicit-scratch"),
        "the resolved-target log must show the explicit value won, not the .env value; \
         captured: {combined}"
    );
    Ok(())
}

#[tokio::test]
async fn no_selected_config_does_not_import_cwd_dotenv() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    std::fs::write(
        tmp.path().join(".env"),
        "MIKE_DATABASE_URL=postgres://from-dotenv@127.0.0.1:6666/claude_proxy\n",
    )?;

    let combined = spawn_and_poll_for(
        tmp.path(),
        "127.0.0.1:19996",
        "127.0.0.1:19997",
        &[],
        "resolved capture database target",
        BOOT_LINE_DEADLINE,
    )
    .await?;

    assert!(
        !combined.contains("process environment value preserved"),
        "no divergence WARN expected when nothing was set beforehand; captured: {combined}"
    );
    assert!(
        combined.contains("resolved capture database target") && !combined.contains("from-dotenv"),
        "an unselected cwd dotenv must not apply; captured: {combined}"
    );
    Ok(())
}
