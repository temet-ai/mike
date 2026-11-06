use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

pub fn strip_policy_for_owned_command(command: &mut Command, cwd: &Path) -> Result<(), String> {
    let explicit = command
        .get_envs()
        .find(|(key, _)| *key == OsStr::new("PATH"));
    let path = match explicit {
        Some((_, value)) => value.map(OsStr::to_os_string),
        None => std::env::var_os("PATH"),
    };
    let Some(path) = path else {
        return Ok(());
    };
    match crate::launch::fleet::cargo_policy::paths::managed_path(&path, cwd)? {
        Some(path) => {
            command.env("PATH", path);
        }
        None => {
            command.env_remove("PATH");
        }
    }
    Ok(())
}
