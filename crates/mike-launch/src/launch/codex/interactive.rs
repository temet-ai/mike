use std::path::{Path, PathBuf};

use crate::launch::cli::runner::Runner;
use crate::launch::{ChannelCheck, LaunchError, LaunchOutput, LaunchSpec, Surface};

pub(crate) async fn prepare(runner: &Runner<'_>, spec: &mut LaunchSpec) -> Result<(), LaunchError> {
    if !runner.ledger.available() {
        return Err(LaunchError::Refused(
            "Codex spawn requires a reachable durable Mike launch ledger".into(),
        ));
    }
    if spec
        .flags
        .run
        .as_deref()
        .is_some_and(|run| run.starts_with("fleet-"))
    {
        return Err(LaunchError::Usage(
            "Codex fleet seats use Mike's managed Codex runtime; ordinary spawn cannot join a fleet run".into(),
        ));
    }
    let inspect = runner
        .inspect_addr
        .as_deref()
        .ok_or_else(|| LaunchError::Failed("Codex spawn requires Mike's inspect address".into()))?;
    let home = canonical_home()?;
    let installer = super::setup::installer().ok_or_else(|| {
        LaunchError::Failed(
            "Codex integration installer is missing; reinstall Mike's shared scripts".into(),
        )
    })?;
    let setup = mike_codex::setup::install(&installer, &home, &http_url(inspect))
        .await
        .map_err(|error| LaunchError::Failed(format!("preparing Codex integration: {error}")))?;
    if !setup.notice.is_empty() {
        eprintln!("mike spawn: {}", setup.notice);
    }
    configure(spec, &home, runner.proxy_addr.as_deref())?;
    Ok(())
}

fn canonical_home() -> Result<PathBuf, LaunchError> {
    let home = super::options::operator_codex_home()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| LaunchError::Failed("CODEX_HOME and HOME are both unset".into()))?;
    std::fs::create_dir_all(&home)
        .map_err(|error| LaunchError::io("creating the Codex home", error))?;
    std::fs::canonicalize(&home).map_err(|error| LaunchError::io("resolving the Codex home", error))
}

fn configure(spec: &mut LaunchSpec, home: &Path, proxy: Option<&str>) -> Result<(), LaunchError> {
    let home = home
        .to_str()
        .ok_or_else(|| LaunchError::Failed("the Codex home path is not valid UTF-8".into()))?;
    let mut args = Vec::new();
    if spec.flags.proxy_env {
        let proxy = proxy.ok_or_else(|| {
            LaunchError::Failed("Codex spawn requires Mike's proxy address".into())
        })?;
        let session = spec.session.ok_or_else(|| {
            LaunchError::Failed("Codex spawn has no Mike session identity".into())
        })?;
        args.extend(mike_codex::interactive::proxy_args(&format!(
            "{}/codex/launch/{session}",
            http_url(proxy)
        )));
    }
    let index = spec
        .args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or_else(|| {
            spec.args
                .len()
                .saturating_sub(usize::from(spec.prompt.is_some()))
        });
    spec.args.splice(index..index, args);
    spec.env.insert("CODEX_HOME".into(), home.into());
    spec.env.insert("MIKE_CODEX_HOME".into(), home.into());
    Ok(())
}

fn http_url(address: &str) -> String {
    let address = address.trim_end_matches('/');
    if address.starts_with("http://") || address.starts_with("https://") {
        address.into()
    } else {
        format!("http://{address}")
    }
}

pub(crate) fn opened(
    mut output: LaunchOutput,
    spec: &LaunchSpec,
    surface: &Surface,
) -> LaunchOutput {
    output.workspace = surface.workspace_ref.clone();
    output.surface = Some(surface.surface_ref.clone());
    output.session = spec.session;
    output.reused = surface.reused;
    output.opened = true;
    output.delivered = None;
    output.confirmed = false;
    output.channel_check = ChannelCheck::Disabled;
    output.brief_mode = spec.brief.as_ref().map(|brief| brief.mode);
    output
}

#[cfg(test)]
#[path = "interactive_tests.rs"]
mod tests;
