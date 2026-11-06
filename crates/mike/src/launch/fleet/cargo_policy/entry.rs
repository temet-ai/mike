use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::launch::fleet::cargo_policy::classifier::{self, Decision};
use crate::launch::fleet::cargo_policy::paths;

pub const REFUSED: i32 = 125;
pub const HEAVY_MESSAGE: &str = "mike fleet: direct heavyweight or unclassified Cargo is disabled in this seat. Queue the intended command explicitly with fleet_check(command) or mike fleet check --command. Nothing was queued or executed. Operator opt-out: seat_settings.env.MIKE_FLEET_CARGO_POLICY=0 for newly launched seats.";
pub const FORMAT_MESSAGE: &str = "mike fleet: cargo fmt is an external Cargo subcommand and is disabled by the narrow Cargo policy. Use scoped rustfmt directly, or explicitly queue an approved formatter command with fleet_check(command). Nothing was queued or executed.";

pub fn dispatch(arguments: &[OsString]) -> Option<i32> {
    if arguments.get(1).is_none_or(|value| value != "fleet")
        || arguments
            .get(2)
            .is_none_or(|value| value != "cargo-policy-exec")
    {
        return None;
    }
    let Some(directory) = arguments.get(3).map(Path::new) else {
        return Some(REFUSED);
    };
    let Some(argv0) = arguments.get(4) else {
        return Some(REFUSED);
    };
    if Path::new(argv0).file_name() != Some(std::ffi::OsStr::new("cargo")) {
        return Some(REFUSED);
    }
    if paths::verify(directory).is_err() {
        eprintln!(
            "mike fleet: Cargo policy directory could not be verified; command was not executed"
        );
        return Some(REFUSED);
    }
    match classifier::classify(&arguments[5..]) {
        Decision::Format => {
            eprintln!("{FORMAT_MESSAGE}");
            Some(REFUSED)
        }
        Decision::HeavyOrUnknown => {
            eprintln!("{HEAVY_MESSAGE}");
            Some(REFUSED)
        }
        Decision::Query => Some(exec_query(directory, argv0, &arguments[5..])),
    }
}

fn exec_query(directory: &Path, argv0: &OsString, arguments: &[OsString]) -> i32 {
    let result = std::env::current_dir()
        .map_err(|_| "caller cwd is unavailable".to_string())
        .and_then(|cwd| {
            let path = std::env::var_os("PATH").ok_or("caller PATH is unavailable")?;
            paths::real_cargo(directory, &path, &cwd)
        });
    let Ok((executable, _search_path)) = result else {
        eprintln!("mike fleet: real Cargo could not be resolved safely; command was not executed");
        return REFUSED;
    };
    let _ = Command::new(executable).arg0(argv0).args(arguments).exec();
    eprintln!("mike fleet: real Cargo exec failed");
    REFUSED
}
