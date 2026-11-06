use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use super::build_admission::{Pending, Permit};
use super::operation::Execution;

pub(super) const SHIMS_ENV: &str = "MIKE_FLEET_SHIMS";
const HEAVY_VERBS: [&str; 15] = [
    "build", "b", "check", "c", "test", "t", "clippy", "nextest", "run", "r", "bench", "doc", "d",
    "llvm-cov", "rustc",
];

pub(super) fn heavy(args: &[OsString]) -> bool {
    args.iter()
        .map_while(|arg| arg.to_str().filter(|arg| *arg != "--"))
        .any(|arg| HEAVY_VERBS.contains(&arg))
}

pub(super) fn path_without(path: &OsStr, shims: &Path) -> OsString {
    std::env::join_paths(std::env::split_paths(path).filter(|entry| entry != shims))
        .unwrap_or_else(|_| path.to_owned())
}

pub fn run(args: Vec<OsString>) -> i32 {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path = match std::env::var_os(SHIMS_ENV) {
        Some(shims) => path_without(&path, Path::new(&shims)),
        None => path,
    };
    let started_at = chrono::Utc::now();
    let heavy = heavy(&args);
    let lock_acquired_at = heavy.then(|| {
        if let Some(directory) = super::build_lock::build_directory(&args, &path) {
            super::build_lock::wait_until_free(&directory);
        }
        chrono::Utc::now()
    });
    let permit = heavy.then(admit_from_project).flatten();
    let admitted_at = chrono::Utc::now();
    let status = std::process::Command::new("cargo")
        .args(&args)
        .env("PATH", path)
        .status();
    drop(permit);
    match status {
        Ok(status) => {
            let code = exit_code(status);
            if heavy {
                super::build_timing::record(
                    &args,
                    (started_at, lock_acquired_at, admitted_at),
                    code,
                );
            }
            code
        }
        Err(err) => {
            eprintln!("mike: could not start cargo: {err}");
            127
        }
    }
}

pub(super) struct CargoResource {
    pub(super) name: String,
    pub(super) capacity: u32,
}

fn find_profile(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .map(|dir| dir.join(super::operation_profile::PROFILE_PATH))
        .find(|path| path.is_file())
}

pub(super) fn cargo_resource(profile: &[u8]) -> Option<CargoResource> {
    let value: serde_json::Value = serde_json::from_slice(profile).ok()?;
    let execution: Execution = serde_json::from_value(value.get("execution")?.clone()).ok()?;
    if !execution.cargo {
        return None;
    }
    let mut uses: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for operation in execution.operations.values() {
        let command = &operation.command;
        if !["cargo", "scripts/cx", "scripts/gate"]
            .iter()
            .any(|word| command.contains(word))
        {
            continue;
        }
        for claim in &operation.resources {
            let entry = uses
                .entry(claim.name().trim().to_string())
                .or_insert((0, u32::MAX));
            entry.0 += 1;
            entry.1 = entry.1.min(claim.capacity().max(1));
        }
    }
    let (name, (_, capacity)) = uses
        .into_iter()
        .max_by(|a, b| a.1.0.cmp(&b.1.0).then_with(|| b.0.cmp(&a.0)))?;
    Some(CargoResource { name, capacity })
}

fn admit_from_project() -> Option<Permit> {
    let cwd = std::env::current_dir().ok()?;
    let bytes = std::fs::read(find_profile(&cwd)?).ok()?;
    let resource = cargo_resource(&bytes)?;
    let root = match super::build_admission_root::prepare() {
        Ok(root) => root,
        Err(err) => {
            eprintln!("mike: not waiting for a build unit ({err})");
            return None;
        }
    };
    match admit(&root, &resource, |in_use| {
        eprintln!(
            "mike: waiting for a build unit of {} ({in_use} of {} in use)",
            resource.name, resource.capacity
        );
    }) {
        Ok(permit) => {
            eprintln!("mike: admitted to a build unit of {}", resource.name);
            Some(permit)
        }
        Err(err) => {
            eprintln!("mike: not waiting for a build unit ({err})");
            None
        }
    }
}

pub(super) fn admit(
    root: &Path,
    resource: &CargoResource,
    mut waiting: impl FnMut(usize),
) -> Result<Permit, String> {
    let directory = super::operation_admission::directory(root, &resource.name)?;
    let mut slots = (0..resource.capacity as usize)
        .map(|slot| Pending::open_slot(&directory, slot).map_err(|err| err.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let token = CancellationToken::new();
    let mut announced = false;
    loop {
        for at in 0..slots.len() {
            if slots[at].try_take(&token).map_err(|err| err.to_string())? {
                return Ok(slots.swap_remove(at).into_permit());
            }
        }
        if !announced {
            announced = true;
            waiting(slots.len());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(unix)]
fn exit_code(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}

#[cfg(not(unix))]
fn exit_code(status: std::process::ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

#[cfg(test)]
#[path = "build_slot_tests.rs"]
mod tests;
