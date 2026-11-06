use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

pub(super) const ENABLED_ENV: &str = "MIKE_FLEET_CARGO_POLICY";
pub(super) const DEFAULT_POLICY: &str = "0";
const PREPARATION_ERROR: &str = "mikeCargoPolicyPreparationError";

#[cfg(unix)]
pub mod cargo_resolution;
#[cfg(unix)]
pub mod classifier;
#[cfg(unix)]
pub mod entry;
#[cfg(unix)]
pub mod install;
#[cfg(unix)]
pub mod managed_environment;
#[cfg(unix)]
pub mod paths;

pub fn dispatch(arguments: &[OsString]) -> Option<i32> {
    #[cfg(unix)]
    {
        entry::dispatch(arguments)
    }
    #[cfg(not(unix))]
    {
        let _ = arguments;
        None
    }
}

pub(super) fn configure(settings: &mut Value, scratch: &Path) {
    #[cfg(unix)]
    if let Err(why) = install::configure(settings, scratch)
        && let Some(fields) = settings.as_object_mut()
    {
        fields.insert(PREPARATION_ERROR.into(), Value::String(why));
    }
    #[cfg(not(unix))]
    let _ = (settings, scratch);
}

pub(super) fn prepare(settings: &Value) -> Result<(), String> {
    if let Some(why) = settings[PREPARATION_ERROR].as_str() {
        return Err(why.into());
    }
    if settings["env"][ENABLED_ENV] != "1" {
        return Ok(());
    }
    #[cfg(unix)]
    {
        install::prepare(
            settings,
            &super::seat_settings::seat_executable().map_err(|_| "Mike executable unavailable")?,
        )
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

pub(super) fn setup(arguments: &[String]) -> Result<Option<(PathBuf, String)>, String> {
    let mut arguments = arguments.iter();
    let mut settings = None;
    while let Some(argument) = arguments.next() {
        let source = if argument == "--settings" {
            arguments.next().map(String::as_str)
        } else {
            argument.strip_prefix("--settings=")
        };
        if let Some(source) = source {
            settings = serde_json::from_str::<Value>(source).ok();
            break;
        }
    }
    let Some(settings) = settings else {
        return Ok(None);
    };
    if let Some(why) = settings[PREPARATION_ERROR].as_str() {
        return Err(why.into());
    }
    if settings["env"][ENABLED_ENV] != "1" {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        let directory = settings["env"][install::DIRECTORY_ENV]
            .as_str()
            .ok_or("Cargo policy directory unavailable")?;
        let arguments = vec!["--settings".into(), settings.to_string()];
        let command =
            install::shell_setup(&arguments).ok_or("Cargo policy activation unavailable")?;
        Ok(Some((PathBuf::from(directory), command)))
    }
    #[cfg(not(unix))]
    {
        Err("Cargo policy activation requires Unix".into())
    }
}

pub(super) fn strip_managed(command: &mut std::process::Command, cwd: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        managed_environment::strip_policy_for_owned_command(command, cwd)
    }
    #[cfg(not(unix))]
    {
        let _ = (command, cwd);
        Ok(())
    }
}

#[cfg(all(unix, feature = "test-seams"))]
pub mod subprocess_cases;

#[cfg(all(unix, feature = "test-seams"))]
pub mod test_seams;
