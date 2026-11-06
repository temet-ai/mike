use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

#[derive(serde::Deserialize, PartialEq, Debug)]
struct OwnedInstance {
    instance_id: String,
    generation: uuid::Uuid,
    pid: u32,
}

struct DaemonGuard {
    data_dir: PathBuf,
}

impl DaemonGuard {
    fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = stop_owned(&self.data_dir);
    }
}

fn state_dir(home: &Path) -> anyhow::Result<PathBuf> {
    Ok(mike::config::paths::InstallationPaths::resolve(Some(&home.join("config.toml")))?.state_dir)
}

fn read_instance(home: &Path) -> anyhow::Result<OwnedInstance> {
    let raw = std::fs::read(state_dir(home)?.join("instance.json"))?;
    let instance: OwnedInstance = serde_json::from_slice(&raw)?;
    anyhow::ensure!(
        instance.pid > 1 && instance.pid <= i32::MAX as u32,
        "invalid owned test pid"
    );
    Ok(instance)
}

fn stop_owned(home: &Path) -> anyhow::Result<()> {
    use std::io::{Read, Write};
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    use std::os::unix::net::UnixStream;

    let instance = read_instance(home)?;
    let path = state_dir(home)?.join("control.sock");
    let metadata = std::fs::symlink_metadata(&path)?;
    anyhow::ensure!(
        metadata.file_type().is_socket()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.permissions().mode() & 0o077 == 0,
        "test control socket is not privately owned; no stop sent"
    );
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = serde_json::json!({"protocol": 1, "instance_id": &instance.instance_id,
        "generation": instance.generation, "boot_fingerprint": null, "action": "stop"});
    stream.write_all(&serde_json::to_vec(&request)?)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut bytes = Vec::new();
    stream.take(8193).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 8192, "oversized test control response");
    let response: serde_json::Value = serde_json::from_slice(&bytes)?;
    let acknowledged: OwnedInstance =
        serde_json::from_value(response["status"]["identity"].clone())?;
    anyhow::ensure!(
        acknowledged == instance && response["status"]["stopping"] == true,
        "owned stop generation was not acknowledged"
    );
    Ok(())
}

fn mike_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mike")
}

fn isolated_home() -> anyhow::Result<tempfile::TempDir> {
    use std::os::unix::fs::PermissionsExt;
    let base = if cfg!(target_os = "macos") {
        "/private/tmp"
    } else {
        "/tmp"
    };
    let directory = tempfile::Builder::new()
        .prefix("mike-cli-")
        .tempdir_in(base)?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    Ok(directory)
}

fn unused_address() -> anyhow::Result<String> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .to_string())
}

fn isolate(command: &mut Command, home: &Path) {
    command
        .arg("--config")
        .arg(home.join("config.toml"))
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env("MIKE_LOG_DIR", home)
        .env("MIKE_DATA_DIR", home)
        .env("MIKE_LAUNCH_AUTO_TRUST", "0")
        .current_dir(home);
}

fn write_capture_script(dir: &Path) -> anyhow::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("capture-claude.sh");
    let body = "#!/usr/bin/env bash\n{\n  echo \"BASE_URL=${ANTHROPIC_BASE_URL}\"\n  echo \"ARGC=$#\"\n  for a in \"$@\"; do echo \"ARG=$a\"; done\n} > \"$CAPTURE_OUT\"\n";
    std::fs::write(&path, body)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

async fn wait_healthy(inspect_addr: &str, deadline: Duration) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()?;
    let url = format!("http://{inspect_addr}/api/health");
    let start = std::time::Instant::now();
    loop {
        if let Ok(resp) = client.get(&url).send().await
            && resp.status().is_success()
        {
            return Ok(());
        }
        if start.elapsed() >= deadline {
            anyhow::bail!("inspect bind {inspect_addr} never became healthy");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn health_ok(inspect_addr: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
    else {
        return false;
    };
    matches!(
        client
            .get(format!("http://{inspect_addr}/api/health"))
            .send()
            .await,
        Ok(resp) if resp.status().is_success()
    )
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    if std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| {
            s.rsplit_once(')')
                .map(|(_, tail)| tail.trim_start().starts_with('Z'))
        })
        == Some(true)
    {
        return false;
    }
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn wait_for_exit(pid: u32, deadline: Duration) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    while pid_alive(pid) {
        if start.elapsed() > deadline {
            anyhow::bail!(
                "pid {pid} did not exit after its owned stop request within {deadline:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

async fn run_wrapper(
    data_dir: &Path,
    capture_script: &Path,
    capture_out: &Path,
    proxy_addr: &str,
    inspect_addr: &str,
    args: &[&str],
) -> anyhow::Result<String> {
    let mut cmd = Command::new(mike_bin());
    isolate(&mut cmd, data_dir);
    cmd.arg("claude").arg("--");
    for a in args {
        cmd.arg(a);
    }
    cmd.current_dir(data_dir);
    cmd.env("MIKE_LISTEN_ADDR", proxy_addr)
        .env("MIKE_INSPECT_ADDR", inspect_addr)
        .env("MIKE_SPA_ENABLED", "0")
        .env("MIKE_PERSIST_ENABLED", "0")
        .env("MIKE_DATA_DIR", data_dir)
        .env("MIKE_CLAUDE_BIN", capture_script)
        .env("CAPTURE_OUT", capture_out)
        .env("RUST_LOG", "warn")
        .env("MIKE_LAUNCH_CLAUDE_ARGS", "")
        .env("MIKE_LAUNCH_CA_CERT", data_dir.join("no-such-ca.pem"));
    let out = cmd.output().await?;
    anyhow::ensure!(
        out.status.success(),
        "wrapper exited non-zero: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[tokio::test]
async fn cold_spawns_surviving_daemon_then_warm_reuses_it() -> anyhow::Result<()> {
    let proxy_addr = &unused_address()?;
    let inspect_addr = &unused_address()?;

    let home = isolated_home()?;
    let data_dir = home.path().to_path_buf();
    let guard = DaemonGuard::new(&data_dir);

    let capture_script = write_capture_script(&data_dir)?;
    let cold_out = data_dir.join("cold-capture.txt");
    let warm_out = data_dir.join("warm-capture.txt");

    let cold_stdout = run_wrapper(
        &data_dir,
        &capture_script,
        &cold_out,
        proxy_addr,
        inspect_addr,
        &["-p", "cold-prompt", "--model", "sonnet"],
    )
    .await?;

    assert!(
        cold_stdout.contains("started server"),
        "cold path should report a freshly-started server; stdout: {cold_stdout}"
    );

    let instance = read_instance(&data_dir)?;
    let pid = instance.pid;

    wait_healthy(inspect_addr, Duration::from_secs(20)).await?;
    assert!(
        pid_alive(pid),
        "daemon (pid {pid}) must survive the wrapper exit"
    );

    let cold_capture = std::fs::read_to_string(&cold_out)?;
    assert!(
        cold_capture.contains(&format!("BASE_URL=http://{proxy_addr}")),
        "claude must be launched with ANTHROPIC_BASE_URL=http://{proxy_addr}; got:\n{cold_capture}"
    );
    let arg_lines: Vec<&str> = cold_capture
        .lines()
        .filter_map(|l| l.strip_prefix("ARG="))
        .collect();
    assert_eq!(
        arg_lines.first().copied(),
        Some("--session-id"),
        "mike must inject its own session id first; got: {arg_lines:?}"
    );
    let session_id = arg_lines.get(1).copied().unwrap_or_default();
    assert!(
        uuid::Uuid::parse_str(session_id).is_ok(),
        "session id must be a uuid; got {session_id:?}"
    );
    assert_eq!(
        &arg_lines[2..],
        ["-p", "cold-prompt", "--model", "sonnet"],
        "forwarded args must follow the injected session id, in order; got: {arg_lines:?}"
    );

    let warm_stdout = run_wrapper(
        &data_dir,
        &capture_script,
        &warm_out,
        proxy_addr,
        inspect_addr,
        &["-p", "warm-prompt"],
    )
    .await?;

    assert!(
        warm_stdout.contains("reusing"),
        "warm path should reuse the running server; stdout: {warm_stdout}"
    );
    assert!(
        !warm_stdout.contains("started server"),
        "warm path must not spawn a second daemon; stdout: {warm_stdout}"
    );
    let instance_after = read_instance(&data_dir)?;
    assert_eq!(
        instance_after, instance,
        "warm path must reuse the same owned generation"
    );
    assert!(
        pid_alive(pid),
        "daemon must still be alive on the warm path"
    );

    let warm_capture = std::fs::read_to_string(&warm_out)?;
    assert!(
        warm_capture.contains(&format!("BASE_URL=http://{proxy_addr}")),
        "warm run must also inject the base URL; got:\n{warm_capture}"
    );

    stop_owned(&data_dir)?;
    wait_for_exit(pid, Duration::from_secs(65)).await?;
    assert!(
        !health_ok(inspect_addr).await,
        "inspect bind must be down after the owned daemon drains and exits"
    );

    drop(guard);
    Ok(())
}

#[tokio::test]
async fn bare_invocation_serves() -> anyhow::Result<()> {
    let proxy_addr = &unused_address()?;
    let inspect_addr = &unused_address()?;
    let spa_addr = &unused_address()?;
    let home = isolated_home()?;

    let mut command = Command::new(mike_bin());
    isolate(&mut command, home.path());
    let mut child = command
        .env("MIKE_LISTEN_ADDR", proxy_addr)
        .env("MIKE_INSPECT_ADDR", inspect_addr)
        .env("MIKE_SPA_BIND", spa_addr)
        .env("MIKE_SPA_ENABLED", "0")
        .env("MIKE_PERSIST_ENABLED", "0")
        .env("RUST_LOG", "warn")
        .kill_on_drop(true)
        .spawn()?;

    let result = wait_healthy(inspect_addr, Duration::from_secs(20)).await;

    let _ = child.start_kill();
    let _ = child.wait().await;

    result?;
    Ok(())
}
