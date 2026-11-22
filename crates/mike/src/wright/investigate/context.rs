use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::admission::Locks;
use super::sandbox::Binaries;
use crate::feature_toggles::FeatureToggles;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

pub(crate) const POLL: Duration = Duration::from_secs(15);
const SLOT: &str = ".claude/worktrees/wright";
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Machine {
    pub(crate) host: String,
    pub(crate) uuid: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Program {
    Claude,
    #[cfg(test)]
    Script(Vec<String>),
}

#[derive(Debug, Clone)]
pub(crate) struct EnginePaths {
    pub(crate) repo: PathBuf,
    pub(crate) slot: PathBuf,
    pub(crate) refresh_slot: bool,
    pub(crate) runs: PathBuf,
    pub(crate) credentials: PathBuf,
    pub(crate) bwrap: PathBuf,
    pub(crate) binaries: Binaries,
    pub(crate) loadavg: PathBuf,
    pub(crate) deploy_lock: PathBuf,
}

impl EnginePaths {
    pub(crate) fn locks(&self) -> Locks<'_> {
        Locks {
            loadavg: &self.loadavg,
            deploy: &self.deploy_lock,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Upstream {
    pub(crate) proxy: String,
    pub(crate) inspect: String,
    pub(crate) client: reqwest::Client,
}

#[derive(Clone)]
pub(crate) struct EngineerContext {
    pub(crate) persist: PersistHandle,
    pub(crate) toggles: Arc<ArcSwap<FeatureToggles>>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) machine: Machine,
    pub(crate) paths: EnginePaths,
    pub(crate) upstream: Upstream,
    pub(crate) poll: Duration,
    pub(crate) program: Program,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

pub(crate) fn is_elf(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == ELF_MAGIC
}

fn on_path(name: &str) -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(name))
                .filter(|p| p.is_file())
                .filter_map(|p| std::fs::canonicalize(p).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn mise_claude() -> Option<PathBuf> {
    let out = std::process::Command::new("mise")
        .args(["which", "claude"])
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    out.status
        .success()
        .then(|| PathBuf::from(text.trim()))
        .and_then(|p| std::fs::canonicalize(p).ok())
}

fn not_a_shim(path: &Path) -> bool {
    let shown = path.to_string_lossy();
    !shown.contains("/mise/shims/") && !shown.contains("/mise/command-wrappers/")
}

fn resolve_claude() -> Result<PathBuf, String> {
    env_path("MIKE_WRIGHT_CLAUDE_BIN")
        .into_iter()
        .chain(mise_claude())
        .chain(on_path("claude").into_iter().filter(|p| not_a_shim(p)))
        .find(|p| is_elf(p))
        .ok_or_else(|| "no single-file claude executable was found; set MIKE_WRIGHT_CLAUDE_BIN to the claude binary (`mise which claude`)".to_owned())
}

fn resolve_mike_mcp(mike: &Path) -> Result<PathBuf, String> {
    env_path("MIKE_WRIGHT_MCP_BIN")
        .into_iter()
        .chain(mike.parent().map(|dir| dir.join("mike-mcp")))
        .chain(on_path("mike-mcp"))
        .find(|p| is_elf(p))
        .ok_or_else(|| {
            "no mike-mcp executable was found; run `make install` or set MIKE_WRIGHT_MCP_BIN"
                .to_owned()
        })
}

fn resolve_repo() -> Result<PathBuf, String> {
    let candidate = env_path("MIKE_WRIGHT_REPO")
        .or_else(|| std::env::current_dir().ok())
        .ok_or("the working directory is unknown")?;
    if candidate.join("crates/mike/Cargo.toml").is_file() {
        return Ok(candidate);
    }
    Err(format!(
        "{} is not mike's checkout; run mike serve from it or set MIKE_WRIGHT_REPO",
        candidate.display()
    ))
}

fn credentials_path() -> Result<PathBuf, String> {
    env_path("CLAUDE_CONFIG_DIR")
        .or_else(|| env_path("HOME").map(|home| home.join(".claude")))
        .map(|dir| dir.join(".credentials.json"))
        .ok_or_else(|| "neither CLAUDE_CONFIG_DIR nor HOME is set".to_owned())
}

fn bwrap_path() -> Result<PathBuf, String> {
    on_path("bwrap")
        .into_iter()
        .chain(Some(PathBuf::from("/usr/bin/bwrap")))
        .find(|p| is_elf(p))
        .ok_or_else(|| {
            "bwrap is not installed; Wright's engineer runs only inside bwrap".to_owned()
        })
}

pub(crate) fn runs_dir() -> Result<PathBuf, String> {
    Ok(crate::machine_identity::default_state_dir()
        .map_err(|e| e.to_string())?
        .join("wright")
        .join("runs"))
}

fn copy_name(live: &Path) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt as _;
    let meta = std::fs::metadata(live)
        .map_err(|e| format!("cannot read the running mike at {}: {e}", live.display()))?;
    Ok(format!("mike-{}-{}-{}", meta.dev(), meta.ino(), meta.len()))
}

fn remove_other_copies(dir: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name != keep && name.to_string_lossy().starts_with("mike-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub(crate) fn binary_copy(live: &Path, dir: &Path) -> Result<PathBuf, String> {
    let name = copy_name(live)?;
    let copy = dir.join(&name);
    if is_elf(&copy) {
        return Ok(copy);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let partial = dir.join(format!(".{name}.partial"));
    std::fs::copy(live, &partial)
        .and_then(|_| std::fs::rename(&partial, &copy))
        .map_err(|e| {
            let _ = std::fs::remove_file(&partial);
            format!("cannot copy the running mike into {}: {e}", dir.display())
        })?;
    remove_other_copies(dir, &name);
    Ok(copy)
}

fn bin_dir(runs: &Path) -> PathBuf {
    runs.parent().unwrap_or(runs).join("bin")
}

pub(crate) fn resolve_paths() -> Result<EnginePaths, String> {
    let repo = resolve_repo()?;
    let installed =
        std::env::current_exe().map_err(|e| format!("mike's own path is unknown: {e}"))?;
    let runs = runs_dir()?;
    let mike = binary_copy(Path::new("/proc/self/exe"), &bin_dir(&runs))?;
    Ok(EnginePaths {
        slot: repo.join(SLOT),
        refresh_slot: true,
        loadavg: PathBuf::from("/proc/loadavg"),
        deploy_lock: repo.join(".deploy").join("deploy.lock"),
        runs,
        credentials: credentials_path()?,
        bwrap: bwrap_path()?,
        binaries: Binaries {
            claude: resolve_claude()?,
            mike_mcp: resolve_mike_mcp(&installed)?,
            mike,
        },
        repo,
    })
}

fn loopback_url(addr: &str) -> String {
    let addr = addr
        .strip_prefix("0.0.0.0:")
        .map_or(addr.to_owned(), |port| format!("127.0.0.1:{port}"));
    format!("http://{addr}")
}

pub(crate) fn upstream(proxy_addr: &str, inspect_addr: &str) -> Result<Upstream, String> {
    let client = mike_base::http_client::builder("context")
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("the relay's HTTP client cannot be built: {e}"))?;
    Ok(Upstream {
        proxy: loopback_url(proxy_addr),
        inspect: loopback_url(inspect_addr),
        client,
    })
}

fn host_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_owned())
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "unknown-host".to_owned())
}

pub(crate) async fn machine() -> Machine {
    Machine {
        host: host_name(),
        uuid: crate::machine_identity::load_default()
            .await
            .ok()
            .map(|m| m.machine_uuid),
    }
}

impl EngineerContext {
    pub(crate) async fn from_state(state: &AppState) -> Result<Self, String> {
        let persist = state
            .persist()
            .cloned()
            .ok_or("mike has no database, so the engineer has nowhere to record its runs")?;
        let config = state.config();
        Ok(Self {
            persist,
            toggles: state.feature_toggle_overlay.clone(),
            shutdown: state.shutdown(),
            machine: machine().await,
            paths: resolve_paths()?,
            upstream: upstream(&config.proxy_listen_addr, &config.inspect_listen_addr)?,
            poll: POLL,
            program: Program::Claude,
        })
    }
}
