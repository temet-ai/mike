use std::path::{Path, PathBuf};

use mike_codex::{ApprovalPolicy, SandboxMode, ThreadPolicy};

const AUTH_FILE: &str = "auth.json";

pub fn seat_policy() -> ThreadPolicy {
    ThreadPolicy {
        sandbox: SandboxMode::WorkspaceWrite,
        approval: ApprovalPolicy::Never,
    }
}

pub fn seat_codex_home(scratch: &Path, run_id: &str) -> PathBuf {
    let run: String = run_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    scratch.join("codex").join(run)
}

pub fn prepare_home(home: &Path, operator_home: Option<&Path>) -> anyhow::Result<()> {
    let failed = |error: std::io::Error| {
        anyhow::anyhow!(
            "could not prepare the Codex home {}: {error}",
            home.display()
        )
    };
    std::fs::create_dir_all(home).map_err(failed)?;
    let Some(source) = operator_home
        .map(|h| h.join(AUTH_FILE))
        .filter(|p| p.exists())
    else {
        return Ok(());
    };
    let link = home.join(AUTH_FILE);
    if link.symlink_metadata().is_ok() {
        return Ok(());
    }
    std::os::unix::fs::symlink(source, link).map_err(failed)
}

pub fn operator_codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")))
}
