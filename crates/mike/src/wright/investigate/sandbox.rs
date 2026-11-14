use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) const INSIDE_HOME: &str = concat!("/", "home/wright");
pub(crate) const INSIDE_WORK: &str = "/work";
pub(crate) const INSIDE_IN: &str = "/wright/in";
pub(crate) const INSIDE_OUT: &str = "/wright/out";
pub(crate) const INSIDE_SOCKET: &str = "/run/wright/relay.sock";
pub(crate) const INSIDE_BIN: &str = "/opt/wright/bin";
pub(crate) const PROXY_LISTEN: &str = "127.0.0.1:9988";
pub(crate) const INSPECT_LISTEN: &str = "127.0.0.1:9989";
const SYSTEM_ROOTS: [&str; 5] = ["/bin", "/sbin", "/lib", "/lib64", "/lib32"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Binaries {
    pub(crate) claude: PathBuf,
    pub(crate) mike: PathBuf,
    pub(crate) mike_mcp: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SystemEntry {
    Bind(PathBuf),
    Symlink { target: PathBuf, path: PathBuf },
}

#[derive(Debug, Clone)]
pub(crate) struct SandboxPlan {
    pub(crate) system: Vec<SystemEntry>,
    pub(crate) work: PathBuf,
    pub(crate) work_access: WorkAccess,
    pub(crate) masked: Vec<PathBuf>,
    pub(crate) input_dir: PathBuf,
    pub(crate) output_dir: PathBuf,
    pub(crate) home_files: Vec<(PathBuf, &'static str)>,
    pub(crate) socket: PathBuf,
    pub(crate) binaries: Binaries,
    pub(crate) env: Vec<(&'static str, String)>,
    pub(crate) command: Vec<OsString>,
}

pub(crate) fn system_layout() -> Vec<SystemEntry> {
    let mut entries = vec![SystemEntry::Bind(PathBuf::from("/usr"))];
    for root in SYSTEM_ROOTS {
        let path = PathBuf::from(root);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_symlink() => {
                if let Ok(target) = std::fs::read_link(&path) {
                    entries.push(SystemEntry::Symlink { target, path });
                }
            }
            Ok(meta) if meta.is_dir() => entries.push(SystemEntry::Bind(path)),
            _ => {}
        }
    }
    entries.push(SystemEntry::Bind(PathBuf::from("/etc")));
    entries
}

pub(crate) fn base_env(run_id: i64) -> Vec<(&'static str, String)> {
    vec![
        ("HOME", INSIDE_HOME.to_owned()),
        ("USER", "wright".to_owned()),
        ("PATH", format!("{INSIDE_BIN}:/usr/bin")),
        ("LANG", "C.UTF-8".to_owned()),
        ("TERM", "dumb".to_owned()),
        ("TMPDIR", "/tmp".to_owned()),
        ("ANTHROPIC_BASE_URL", format!("http://{PROXY_LISTEN}")),
        ("MIKE_INSPECT_URL", format!("http://{INSPECT_LISTEN}")),
        ("DISABLE_AUTOUPDATER", "1".to_owned()),
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1".to_owned()),
        ("MIKE_WRIGHT_RUN", run_id.to_string()),
    ]
}

fn push(args: &mut Vec<OsString>, parts: &[&str]) {
    args.extend(parts.iter().map(OsString::from));
}

fn bind(args: &mut Vec<OsString>, flag: &str, source: &Path, dest: &str) {
    args.push(flag.into());
    args.push(source.as_os_str().to_owned());
    args.push(dest.into());
}

fn inside(base: &str, relative: &Path) -> String {
    format!("{base}/{}", relative.to_string_lossy())
}

fn system_args(args: &mut Vec<OsString>, system: &[SystemEntry]) {
    for entry in system {
        match entry {
            SystemEntry::Bind(path) => {
                bind(args, "--ro-bind", path, &path.to_string_lossy());
            }
            SystemEntry::Symlink { target, path } => {
                args.push("--symlink".into());
                args.push(target.as_os_str().to_owned());
                args.push(path.as_os_str().to_owned());
            }
        }
    }
}

fn home_args(args: &mut Vec<OsString>, plan: &SandboxPlan) {
    push(args, &["--tmpfs", INSIDE_HOME]);
    push(args, &["--dir", &format!("{INSIDE_HOME}/.claude")]);
    for (source, relative) in &plan.home_files {
        bind(
            args,
            "--ro-bind",
            source,
            &inside(INSIDE_HOME, Path::new(relative)),
        );
    }
    let bins = &plan.binaries;
    bind(
        args,
        "--ro-bind",
        &bins.claude,
        &format!("{INSIDE_BIN}/claude"),
    );
    bind(args, "--ro-bind", &bins.mike, &format!("{INSIDE_BIN}/mike"));
    bind(
        args,
        "--ro-bind",
        &bins.mike_mcp,
        &format!("{INSIDE_BIN}/mike-mcp"),
    );
}

fn work_args(args: &mut Vec<OsString>, plan: &SandboxPlan) {
    let flag = match plan.work_access {
        WorkAccess::ReadOnly => "--ro-bind",
        WorkAccess::ReadWrite => "--bind",
    };
    bind(args, flag, &plan.work, INSIDE_WORK);
    for masked in &plan.masked {
        bind(
            args,
            "--ro-bind",
            Path::new("/dev/null"),
            &inside(INSIDE_WORK, masked),
        );
    }
    bind(args, "--ro-bind", &plan.input_dir, INSIDE_IN);
    bind(args, "--bind", &plan.output_dir, INSIDE_OUT);
    bind(args, "--bind", &plan.socket, INSIDE_SOCKET);
}

pub(crate) fn bwrap_args(plan: &SandboxPlan) -> Vec<OsString> {
    let mut args = Vec::new();
    push(
        &mut args,
        &[
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
            "--cap-drop",
            "ALL",
            "--hostname",
            "wright",
        ],
    );
    system_args(&mut args, &plan.system);
    push(
        &mut args,
        &["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"],
    );
    home_args(&mut args, plan);
    work_args(&mut args, plan);
    push(&mut args, &["--remount-ro", "/", "--chdir", INSIDE_WORK]);
    for (key, value) in &plan.env {
        push(&mut args, &["--setenv", key, value]);
    }
    args.push("--".into());
    args.extend(plan.command.iter().cloned());
    args
}

pub(crate) fn relay_command(inner: Vec<OsString>) -> Vec<OsString> {
    let mut command: Vec<OsString> = [
        format!("{INSIDE_BIN}/mike"),
        "wright".to_owned(),
        "sandbox-relay".to_owned(),
        "--socket".to_owned(),
        INSIDE_SOCKET.to_owned(),
        "--listen".to_owned(),
        PROXY_LISTEN.to_owned(),
        "--listen".to_owned(),
        INSPECT_LISTEN.to_owned(),
        "--".to_owned(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    command.extend(inner);
    command
}

fn is_env_file(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with(".env")
}

pub(crate) fn env_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        for entry in std::fs::read_dir(root.join(&relative))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = relative.join(entry.file_name());
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && is_env_file(&entry.file_name()) {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}
