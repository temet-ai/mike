use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;

use tokio_util::sync::CancellationToken;

use super::build_admission::{Pending, Permit};
use super::operation::{ResourceClaim, TimedCommand};

pub(super) const SHIMS_ENV: &str = "MIKE_FLEET_SHIMS";
pub(super) const DECLARED_ENV: &str = "MIKE_FLEET_TIMED_COMMANDS";

pub(super) fn path_without(path: &OsStr, shims: &Path) -> OsString {
    std::env::join_paths(std::env::split_paths(path).filter(|entry| entry != shims))
        .unwrap_or_else(|_| path.to_owned())
}

pub(super) fn declared_in(text: &str, program: &OsStr) -> Option<TimedCommand> {
    let mut declared: BTreeMap<String, TimedCommand> = serde_json::from_str(text).ok()?;
    declared.remove(program.to_str()?)
}

fn measured(program: &OsStr, args: &[OsString]) -> Option<TimedCommand> {
    let text = std::env::var(DECLARED_ENV).ok()?;
    declared_in(&text, program)
        .filter(|timed| timed.measures(args.iter().filter_map(|arg| arg.to_str())))
}

pub fn run(program: OsString, args: Vec<OsString>) -> i32 {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path = match std::env::var_os(SHIMS_ENV) {
        Some(shims) => path_without(&path, Path::new(&shims)),
        None => path,
    };
    let started_at = chrono::Utc::now();
    let timed = measured(&program, &args);
    let permits = timed.as_ref().map(admit_declared).unwrap_or_default();
    let admitted_at = chrono::Utc::now();
    let status = std::process::Command::new(&program)
        .args(&args)
        .env("PATH", path)
        .status();
    drop(permits);
    let name = program.to_string_lossy();
    match status {
        Ok(status) => {
            let code = exit_code(status);
            if timed.is_some() {
                super::build_timing::record(&name, &args, (started_at, admitted_at), code);
            }
            code
        }
        Err(err) => {
            eprintln!("mike: could not start {name}: {err}");
            127
        }
    }
}

fn admit_declared(timed: &TimedCommand) -> Vec<Permit> {
    if timed.resources.is_empty() {
        return Vec::new();
    }
    let root = match super::build_admission_root::prepare() {
        Ok(root) => root,
        Err(err) => {
            eprintln!("mike: not waiting for a declared resource ({err})");
            return Vec::new();
        }
    };
    let mut claims: Vec<&ResourceClaim> = timed.resources.iter().collect();
    claims.sort_by_key(|claim| claim.name().trim().to_string());
    claims
        .into_iter()
        .filter_map(|claim| admit_announced(&root, claim))
        .collect()
}

fn admit_announced(root: &Path, claim: &ResourceClaim) -> Option<Permit> {
    let name = claim.name().trim();
    let admitted = admit(root, claim, |in_use| {
        eprintln!("mike: waiting for a unit of {name} ({in_use} in use)");
    });
    match admitted {
        Ok(permit) => {
            eprintln!("mike: admitted to a unit of {name}");
            Some(permit)
        }
        Err(err) => {
            eprintln!("mike: not waiting for a unit of {name} ({err})");
            None
        }
    }
}

pub(super) fn admit(
    root: &Path,
    claim: &ResourceClaim,
    mut waiting: impl FnMut(usize),
) -> Result<Permit, String> {
    let directory = super::operation_admission::directory(root, claim.name().trim())?;
    let mut slots = (0..claim.capacity().max(1) as usize)
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
#[path = "timed_command_tests.rs"]
mod tests;
