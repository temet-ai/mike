use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

const LOCK_FILES: [&str; 2] = [".cargo-build-lock", ".cargo-lock"];
const POLL: Duration = Duration::from_millis(200);

pub(super) fn profile_directory(args: &[OsString]) -> String {
    let words: Vec<&str> = args
        .iter()
        .map_while(|arg| arg.to_str().filter(|arg| *arg != "--"))
        .collect();
    let mut profile = None;
    for (at, word) in words.iter().enumerate() {
        if *word == "--release" || *word == "-r" {
            profile = Some("release".to_string());
        } else if *word == "--profile" {
            profile = words.get(at + 1).map(|name| (*name).to_string());
        } else if let Some(name) = word.strip_prefix("--profile=") {
            profile = Some(name.to_string());
        }
    }
    match profile.as_deref() {
        None | Some("dev" | "test") => "debug".to_string(),
        Some("bench") => "release".to_string(),
        Some(name) => name.to_string(),
    }
}

fn target_argument(args: &[OsString]) -> Option<PathBuf> {
    let words: Vec<&str> = args
        .iter()
        .map_while(|arg| arg.to_str().filter(|arg| *arg != "--"))
        .collect();
    words.iter().enumerate().find_map(|(at, word)| {
        if *word == "--target-dir" {
            words.get(at + 1).map(PathBuf::from)
        } else {
            word.strip_prefix("--target-dir=").map(PathBuf::from)
        }
    })
}

fn metadata_target(path: &std::ffi::OsStr) -> Option<PathBuf> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .env("PATH", path)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    value
        .get("build_directory")
        .or_else(|| value.get("target_directory"))
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
}

pub(super) fn build_directory(args: &[OsString], path: &std::ffi::OsStr) -> Option<PathBuf> {
    let root = target_argument(args)
        .or_else(|| std::env::var_os("CARGO_BUILD_BUILD_DIR").map(PathBuf::from))
        .or_else(|| std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from))
        .or_else(|| metadata_target(path))?;
    let root = if root.is_absolute() {
        root
    } else {
        std::env::current_dir().ok()?.join(root)
    };
    Some(root.join(profile_directory(args)))
}

pub(super) fn held(directory: &Path) -> bool {
    LOCK_FILES.iter().any(|name| {
        let Ok(file) = std::fs::File::open(directory.join(name)) else {
            return false;
        };
        match file.try_lock() {
            Ok(()) => {
                let _ = file.unlock();
                false
            }
            Err(std::fs::TryLockError::WouldBlock) => true,
            Err(std::fs::TryLockError::Error(_)) => false,
        }
    })
}

pub(super) fn wait_until_free(directory: &Path) {
    let mut announced = false;
    while held(directory) {
        if !announced {
            announced = true;
            eprintln!(
                "mike: waiting for cargo's build directory lock on {} before taking a build unit",
                directory.display()
            );
        }
        std::thread::sleep(POLL);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BuildWaits {
    pub commands: usize,
    pub lock_wait_minutes: f64,
    pub unit_wait_minutes: f64,
    pub compile_minutes: f64,
}

fn minutes(from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>) -> f64 {
    ((to - from).as_seconds_f64() / 60.0).max(0.0)
}

pub fn build_waits(timings: &[super::build_timing::Timing]) -> BuildWaits {
    let mut waits = BuildWaits::default();
    for timing in timings {
        let locked = timing.lock_acquired_at.unwrap_or(timing.started_at);
        let admitted = timing.admitted_at.unwrap_or(locked);
        waits.commands += 1;
        waits.lock_wait_minutes += minutes(timing.started_at, locked);
        waits.unit_wait_minutes += minutes(locked, admitted);
        waits.compile_minutes += minutes(admitted, timing.finished_at);
    }
    waits
}

pub fn add_to_record(document: &mut serde_json::Value, waits: BuildWaits) {
    let round = |value: f64| (value * 10.0).round() / 10.0;
    let resource = round(waits.lock_wait_minutes + waits.unit_wait_minutes);
    document["build_waits"] = serde_json::json!({
        "commands": waits.commands,
        "lock_wait_minutes": round(waits.lock_wait_minutes),
        "unit_wait_minutes": round(waits.unit_wait_minutes),
        "compile_minutes": round(waits.compile_minutes),
    });
    if let Some(faults) = document
        .get_mut("fault_minutes")
        .and_then(serde_json::Value::as_object_mut)
    {
        let total = faults
            .get("total")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_default();
        faults.insert("resource_wait".into(), serde_json::json!(resource));
        faults.insert("total".into(), serde_json::json!(round(total + resource)));
    }
}

#[cfg(test)]
#[path = "build_lock_tests.rs"]
mod tests;
