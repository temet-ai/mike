use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::launch::claude_args;
use super::sandbox::{
    INSIDE_OUT, INSIDE_SOCKET, INSIDE_WORK, SandboxPlan, WorkAccess, base_env, bwrap_args,
    env_files, relay_command, system_layout,
};
use super::test_support::{BWRAP, bwrap_missing, paths};
use super::{RunMode, RunRequest};

fn strings(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

fn plan(root: &Path, command: Vec<OsString>) -> anyhow::Result<SandboxPlan> {
    let engine = paths(root)?;
    let run = root.join("runs/7");
    for dir in ["in", "out", "home", "sock"] {
        std::fs::create_dir_all(run.join(dir))?;
    }
    let mut home_files = Vec::new();
    for (name, inside) in [
        ("credentials.json", ".claude/.credentials.json"),
        ("settings.json", ".claude/settings.json"),
        ("mcp.json", "mcp.json"),
    ] {
        std::fs::write(run.join("home").join(name), "{}")?;
        home_files.push((run.join("home").join(name), inside));
    }
    let socket = run.join("sock/relay.sock");
    std::fs::write(&socket, "")?;
    Ok(SandboxPlan {
        system: system_layout(),
        masked: env_files(&engine.slot)?,
        work: engine.slot,
        work_access: WorkAccess::ReadOnly,
        input_dir: run.join("in"),
        output_dir: run.join("out"),
        home_files,
        socket,
        binaries: engine.binaries,
        env: base_env(7),
        command,
    })
}

fn triples<'a>(args: &'a [String], flag: &str) -> Vec<(&'a str, &'a str)> {
    args.windows(3)
        .filter(|w| w[0] == flag)
        .map(|w| (w[1].as_str(), w[2].as_str()))
        .collect()
}

#[test]
fn sandbox_args_unshare_everything_and_clear_the_environment() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let args = strings(&bwrap_args(&plan(
        tmp.path(),
        vec!["/usr/bin/true".into()],
    )?));
    for flag in [
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--clearenv",
    ] {
        assert!(args.iter().any(|a| a == flag), "missing {flag}");
    }
    assert!(!args.iter().any(|a| a.starts_with("--share")));
    let clear = args.iter().position(|a| a == "--clearenv");
    let first_env = args.iter().position(|a| a == "--setenv");
    assert!(clear < first_env);
    Ok(())
}

#[test]
fn sandbox_args_make_only_out_and_the_relay_socket_writable() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let args = strings(&bwrap_args(&plan(
        tmp.path(),
        vec!["/usr/bin/true".into()],
    )?));
    let writable: BTreeSet<&str> = triples(&args, "--bind")
        .into_iter()
        .map(|(_, d)| d)
        .collect();
    assert_eq!(writable, BTreeSet::from([INSIDE_OUT, INSIDE_SOCKET]));
    assert!(triples(&args, "--ro-bind").contains(&(
        tmp.path().join("slot").to_string_lossy().as_ref(),
        INSIDE_WORK
    )));
    Ok(())
}

#[test]
fn sandbox_args_mask_every_env_file_with_dev_null() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let args = strings(&bwrap_args(&plan(
        tmp.path(),
        vec!["/usr/bin/true".into()],
    )?));
    let masks: BTreeSet<&str> = triples(&args, "--ro-bind")
        .into_iter()
        .filter(|(src, _)| *src == "/dev/null")
        .map(|(_, dest)| dest)
        .collect();
    assert_eq!(
        masks,
        BTreeSet::from(["/work/.env", "/work/crates/sub/.env.local"])
    );
    Ok(())
}

#[test]
fn sandbox_args_bind_no_secret_or_host_state_paths() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let args = strings(&bwrap_args(&plan(
        tmp.path(),
        vec!["/usr/bin/true".into()],
    )?));
    let run_home = tmp.path().join("runs/7/home");
    let mut sources = triples(&args, "--ro-bind");
    sources.extend(triples(&args, "--bind"));
    for (source, dest) in sources {
        for banned in [
            ".ssh",
            ".config/gh",
            "docker.sock",
            ".git",
            ".deploy",
            ".env",
        ] {
            assert!(
                !source.contains(banned),
                "{source} -> {dest} binds {banned}"
            );
        }
        if dest.starts_with("/home/wright") {
            assert!(
                Path::new(source).starts_with(&run_home),
                "{source} is not the run's copy"
            );
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    assert!(!home.is_empty());
    assert!(!args.iter().any(|a| a == &home), "the host home is bound");
    Ok(())
}

#[test]
fn sandbox_env_carries_only_the_run_variables() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let args = strings(&bwrap_args(&plan(
        tmp.path(),
        vec!["/usr/bin/true".into()],
    )?));
    let keys: BTreeSet<&str> = triples(&args, "--setenv")
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    let expected: BTreeSet<&str> = base_env(7).iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, expected);
    for (key, value) in triples(&args, "--setenv") {
        let lower = value.to_ascii_lowercase();
        assert!(!lower.contains("postgres"), "{key} carries a database URL");
        assert!(!key.contains("DATABASE") && !key.contains("TOKEN") && !key.contains("KEY"));
    }
    Ok(())
}

#[test]
fn claude_args_restrict_tools_to_read_and_mcp_to_mike() {
    let request = RunRequest {
        mode: RunMode::Investigate,
        problem_id: Some(1),
        brief: "brief".to_owned(),
        inputs: Vec::new(),
    };
    let session = Uuid::new_v4();
    let args = strings(&relay_command(claude_args(&request, session)));
    let after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1).cloned())
    };
    assert_eq!(after("--tools").as_deref(), Some("Read,Glob,Grep,Bash"));
    assert_eq!(after("--permission-mode").as_deref(), Some("dontAsk"));
    assert_eq!(after("--session-id"), Some(session.to_string()));
    assert_eq!(after("--setting-sources").as_deref(), Some("user"));
    assert!(args.iter().any(|a| a == "--strict-mcp-config"));
    assert_eq!(
        args[..3],
        ["/opt/wright/bin/mike", "wright", "sandbox-relay"]
    );
}

const PROBE: &str = r#"
probe() { if eval "$2" >/dev/null 2>&1; then echo "$1=yes"; else echo "$1=no"; fi; }
{
probe database 'exec 3<>/dev/tcp/127.0.0.1/5440'
probe host_port "exec 3<>/dev/tcp/127.0.0.1/$HOST_PORT"
probe write_work 'echo x > /work/new-file'
probe write_in 'echo x > /wright/in/new-file'
probe write_home_bin 'echo x > /opt/wright/bin/new-file'
probe write_out 'echo x > /wright/out/new-file'
probe env_secret '[ -s /work/.env ]'
probe env_nested_secret '[ -s /work/crates/sub/.env.local ]'
probe readme '[ -s /work/README.md ]'
probe ssh '[ -e "$HOME/.ssh" ]'
probe database_url '[ -n "${DATABASE_URL:-}${MIKE_TEST_ADMIN_URL:-}" ]'
} > /wright/out/probe.txt
"#;

#[tokio::test]
async fn real_sandbox_blocks_network_and_writes_outside_out() -> anyhow::Result<()> {
    if let Some(reason) = bwrap_missing() {
        eprintln!("{reason}");
        return Ok(());
    }
    let host = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = host.local_addr()?.port();
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
    let tmp = tempfile::tempdir()?;
    let script = PROBE.replace("$HOST_PORT", &port.to_string());
    let mut plan = plan(
        tmp.path(),
        vec!["/usr/bin/bash".into(), "-c".into(), script.into()],
    )?;
    plan.socket = tmp.path().join("runs/7/sock/relay.sock");
    let status = tokio::process::Command::new(BWRAP)
        .args(bwrap_args(&plan))
        .env_clear()
        .status()
        .await?;
    assert!(status.success(), "bwrap failed: {status}");
    let out = std::fs::read_to_string(tmp.path().join("runs/7/out/probe.txt"))?;
    let expected = "database=no\nhost_port=no\nwrite_work=no\nwrite_in=no\nwrite_home_bin=no\nwrite_out=yes\nenv_secret=no\nenv_nested_secret=no\nreadme=yes\nssh=no\ndatabase_url=no\n";
    assert_eq!(out, expected);
    let slot: PathBuf = tmp.path().join("slot");
    assert!(!slot.join("new-file").exists());
    assert!(std::fs::read_to_string(slot.join(".env"))?.contains("secret"));
    Ok(())
}
