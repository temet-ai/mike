use std::path::{Path, PathBuf};
use std::process::Command;

const UNKNOWN: &str = "unknown";
const MBX_BUILD: &str = "mbx-build";
const DEV_BUILD: &str = "dev-build";

/// Stamps the build with its source identity.
///
/// Only a build that ships or runs as the stack carries the real commit: the
/// release profile (releases, `make install`), or any profile with
/// `MIKE_BUILD_IDENTITY=git` (the deploy and run scripts). Every other build —
/// the checks and tests of the edit loop — gets a fixed identity and watches no
/// git file, so a commit or branch switch does not recompile the crate.
pub fn emit_build_identity(manifest_dir: &Path) {
    let repo_root = manifest_dir.join("../..");

    println!("cargo:rerun-if-env-changed=MIKE_BUILD_CACHE");
    println!("cargo:rerun-if-env-changed=MIKE_BUILD_IDENTITY");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    // Without any rerun-if-changed, cargo would rerun the script on every file
    // change in the package; anchor it to this file instead.
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("../build_identity.rs").display()
    );
    if std::env::var("MIKE_BUILD_CACHE").as_deref() == Ok("mbx") {
        emit_fixed_identity(MBX_BUILD);
        return;
    }
    let shipped = std::env::var("PROFILE").as_deref() == Ok("release")
        || std::env::var("MIKE_BUILD_IDENTITY").as_deref() == Ok("git");
    if !shipped {
        emit_fixed_identity(DEV_BUILD);
        return;
    }
    emit_git_identity(&repo_root);
}

fn emit_fixed_identity(value: &str) {
    for (key, value) in [
        ("MIKE_BUILD_SHA", value),
        ("MIKE_BUILD_SHA_SHORT", value),
        ("MIKE_BUILD_DIRTY", "false"),
        ("MIKE_BUILD_TIME", value),
    ] {
        println!("cargo::rustc-env={key}={value}");
    }
}

fn emit_git_identity(repo_root: &Path) {
    let sha = git_output(repo_root, &["rev-parse", "HEAD"]);
    let short = git_output(repo_root, &["rev-parse", "--short", "HEAD"]);
    let dirty = match git_output(
        repo_root,
        &["status", "--porcelain", "--untracked-files=no"],
    ) {
        Some(out) => !out.is_empty(),
        None => true,
    };

    println!(
        "cargo::rustc-env=MIKE_BUILD_SHA={}",
        sha.as_deref().unwrap_or(UNKNOWN)
    );
    println!(
        "cargo::rustc-env=MIKE_BUILD_SHA_SHORT={}",
        short.as_deref().unwrap_or(UNKNOWN)
    );
    println!("cargo::rustc-env=MIKE_BUILD_DIRTY={dirty}");
    println!("cargo::rustc-env=MIKE_BUILD_TIME={}", build_timestamp());

    for path in git_head_watch_paths(repo_root) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn git_output(repo_root: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    let out = command
        .current_dir(repo_root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["-c", "core.fsmonitor=false", "-c", "protocol.allow=never"])
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

fn git_head_watch_paths(repo_root: &Path) -> Vec<PathBuf> {
    let mut names = vec![
        "HEAD".to_string(),
        "index".to_string(),
        "packed-refs".to_string(),
    ];
    if let Some(head_ref) = git_output(repo_root, &["symbolic-ref", "--quiet", "HEAD"]) {
        names.push(head_ref);
    }
    names
        .iter()
        .filter_map(|name| git_output(repo_root, &["rev-parse", "--git-path", name]))
        .map(|value| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            }
        })
        .filter(|path| path.exists())
        .collect()
}

fn build_timestamp() -> String {
    let secs = match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(value) => parse_source_date_epoch(&value),
        Err(std::env::VarError::NotPresent) => {
            match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(duration) => match i64::try_from(duration.as_secs()) {
                    Ok(secs) => secs,
                    Err(_) => return UNKNOWN.to_string(),
                },
                Err(_) => return UNKNOWN.to_string(),
            }
        }
        Err(_) => panic!("SOURCE_DATE_EPOCH must contain an unsigned decimal UTC epoch"),
    };
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn parse_source_date_epoch(value: &str) -> i64 {
    let valid = !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    let parsed = value
        .parse::<i64>()
        .ok()
        .filter(|secs| *secs <= 253_402_300_799);
    match (valid, parsed) {
        (true, Some(secs)) => secs,
        _ => panic!("SOURCE_DATE_EPOCH must be an unsigned decimal UTC epoch through year 9999"),
    }
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
