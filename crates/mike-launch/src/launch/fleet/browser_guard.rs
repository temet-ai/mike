#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

pub const ENABLED_ENV: &str = "MIKE_FLEET_BROWSER_GUARD";
const DIRECTORY_ENV: &str = "MIKE_FLEET_BROWSER_GUARD_BIN";
const WRAPPER: &str = include_str!("browser_guard_wrapper.sh");

pub fn configure(settings: &mut Value, scratch: &Path) {
    if !cfg!(unix) || settings["env"][ENABLED_ENV] != "1" {
        return;
    }
    let directory = scratch.join("browser-guard");
    settings["env"][DIRECTORY_ENV] = json!(directory.to_str());
    if let Some(path) = settings["env"]["PATH"].as_str() {
        settings["env"]["PATH"] = json!(format!("{}:{path}", directory.display()));
    }
    append_hook(settings, &directory);
}

fn append_hook(settings: &mut Value, directory: &Path) {
    if settings["disableAllHooks"] == true {
        return;
    }
    let Some(fields) = settings.as_object_mut() else {
        return;
    };
    let Some(hooks) = fields
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
    else {
        return;
    };
    let Some(start) = hooks
        .entry("SessionStart")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    else {
        return;
    };
    let fragment = format!(". {}", quote(&directory.join("activate").to_string_lossy()));
    let handler = json!({
        "hooks": [{
            "type": "command",
            "command": format!(
                "test -n \"${{CLAUDE_ENV_FILE:-}}\" && printf '\\n%s\\n' {} >> \"$CLAUDE_ENV_FILE\"",
                quote(&fragment)
            ),
            "timeout": 5,
        }],
    });
    if !start.contains(&handler) {
        start.push(handler);
    }
}

pub fn directory(args: &[String]) -> Option<PathBuf> {
    let settings = inline_settings(args)?;
    if settings["env"][ENABLED_ENV] != "1" {
        return None;
    }
    settings["env"][DIRECTORY_ENV].as_str().map(PathBuf::from)
}

pub fn shell_setup(args: &[String]) -> Option<String> {
    Some(format!(
        ". {}",
        quote(&directory(args)?.join("activate").to_string_lossy())
    ))
}

fn inline_settings(args: &[String]) -> Option<Value> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--settings" {
            return serde_json::from_str(iter.next()?).ok();
        }
        if let Some(value) = arg.strip_prefix("--settings=") {
            return serde_json::from_str(value).ok();
        }
    }
    None
}

pub fn terminal_command(program: &str, args: &[String]) -> Vec<String> {
    let Some(setup) = shell_setup(args) else {
        return std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect();
    };
    [
        "/bin/sh".into(),
        "-c".into(),
        format!("{setup} && exec \"$@\""),
        "mike-fleet".into(),
        program.into(),
    ]
    .into_iter()
    .chain(args.iter().cloned())
    .collect()
}

pub fn prepare(settings: &Value) -> Result<(), String> {
    if !cfg!(unix) || settings["env"][ENABLED_ENV] != "1" {
        return Ok(());
    }
    let directory = settings["env"][DIRECTORY_ENV]
        .as_str()
        .ok_or_else(|| "fleet browser guard requires a UTF-8 scratch directory".to_string())?;
    install(Path::new(directory))
        .map_err(|error| format!("cannot prepare fleet browser guard: {error}"))
}

#[cfg(unix)]
fn install(directory: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    if !directory.is_absolute() || directory.to_str().is_none_or(|path| path.contains(':')) {
        return Err(std::io::Error::other(
            "browser guard needs an absolute UTF-8 path without ':'",
        ));
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    let metadata = std::fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(std::io::Error::other(
            "browser guard directory must be private and not a symlink",
        ));
    }
    for name in ["xdg-open", "gio", "open"] {
        write_script(&directory.join(name), WRAPPER)?;
    }
    write_script(
        &directory.join(".mike-browser-guard"),
        "mike-fleet-browser-guard-v1\n",
    )?;
    let activation = activation(directory);
    write_script(&directory.join("activate"), &activation)
}

#[cfg(not(unix))]
fn install(_directory: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("fleet browser guard requires Unix"))
}

fn activation(directory: &Path) -> String {
    let directory = quote(&directory.to_string_lossy());
    format!(
        "for _mike_opener in xdg-open gio open; do\n  if [ ! -x {directory}/\"$_mike_opener\" ]; then\n    printf '%s\\n' 'mike fleet: browser guard is unavailable; refusing launch' >&2\n    return 1\n  fi\ndone\nunset _mike_opener\nexport PATH={directory}:\"${{PATH-}}\"\n"
    )
}

#[cfg(unix)]
fn write_script(path: &Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("script has no parent"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(text.as_bytes())?;
    staged
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o700))?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(any(test, feature = "test-seams"))]
#[path = "browser_guard_cases.rs"]
pub mod cases;

#[cfg(test)]
#[path = "browser_guard_tests.rs"]
mod tests;
