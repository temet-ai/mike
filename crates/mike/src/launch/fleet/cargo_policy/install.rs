use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::launch::fleet::cargo_policy::paths::{POLICY_MARKER, POLICY_VERSION};

pub const ENABLED_ENV: &str = "MIKE_FLEET_CARGO_POLICY";
pub const DIRECTORY_ENV: &str = "MIKE_FLEET_CARGO_POLICY_BIN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Configured {
    Disabled,
    Prepared(PathBuf),
}

pub fn configure(settings: &mut Value, scratch: &Path) -> Result<Configured, String> {
    let directory = scratch.join("cargo-policy/bin");
    let mut updated = settings.clone();
    let fields = updated
        .as_object_mut()
        .ok_or("fleet settings must be an object")?;
    let environment = fields
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("fleet environment must be an object")?;
    match environment.get(ENABLED_ENV).and_then(Value::as_str) {
        Some("0") => return Ok(Configured::Disabled),
        None | Some("1") => {}
        Some(_) => return Err("Cargo policy setting must be the string 0 or 1".into()),
    }
    if environment
        .get(ENABLED_ENV)
        .is_some_and(|value| !value.is_string())
    {
        return Err("Cargo policy setting must be a string".into());
    }
    let path = supported_path(&directory)?;
    if environment
        .get(DIRECTORY_ENV)
        .is_some_and(|value| value.as_str() != Some(path))
    {
        return Err("operator Cargo policy directory conflicts with the prepared directory".into());
    }
    environment.insert(ENABLED_ENV.into(), json!("1"));
    environment.insert(DIRECTORY_ENV.into(), json!(path));
    if let Some(base) = environment.get("PATH") {
        let base = base.as_str().ok_or("operator PATH must be a string")?;
        if !base.starts_with(&format!("{path}:")) && base != path {
            environment.insert("PATH".into(), json!(format!("{path}:{base}")));
        }
    }
    append_hook(&mut updated, &directory)?;
    *settings = updated;
    Ok(Configured::Prepared(directory))
}

pub fn prepare(settings: &Value, executable: &Path) -> Result<(), String> {
    if settings["env"][ENABLED_ENV] == "0" || settings["env"][ENABLED_ENV].is_null() {
        return Ok(());
    }
    if settings["env"][ENABLED_ENV] != "1" {
        return Err("Cargo policy setting must be 0 or 1".into());
    }
    let directory = settings["env"][DIRECTORY_ENV]
        .as_str()
        .ok_or("Cargo policy has no supported installation directory")?;
    install(Path::new(directory), executable)
}

pub fn install(directory: &Path, executable: &Path) -> Result<(), String> {
    supported_path(directory)?;
    let metadata = executable
        .metadata()
        .map_err(|_| "Cargo policy helper executable is unavailable")?;
    if !metadata.is_file() || metadata.mode() & 0o111 == 0 {
        return Err("Cargo policy helper is not an executable file".into());
    }
    let executable = supported_path(executable)?;
    let root = directory
        .parent()
        .ok_or("Cargo policy directory has no parent")?;
    create_private(root)?;
    create_private(directory)?;
    let canonical = directory
        .canonicalize()
        .map_err(|_| "Cargo policy directory cannot be resolved")?;
    if canonical != directory {
        return Err("Cargo policy installation path must be canonical".into());
    }
    write_once(&directory.join(POLICY_MARKER), POLICY_VERSION, 0o600)?;
    write_once(
        &directory.join(".mike-cargo-shim"),
        b"mike-cargo-shim-v1\n",
        0o600,
    )?;
    let wrapper = format!(
        "#!/bin/sh\nexec {} fleet cargo-policy-exec {} \"$0\" \"$@\"\n",
        quote(executable),
        quote(supported_path(directory)?)
    );
    write_once(&directory.join("cargo"), wrapper.as_bytes(), 0o700)?;
    write_once(
        &directory.join("activate"),
        activation(directory)?.as_bytes(),
        0o600,
    )?;
    crate::launch::fleet::cargo_policy::paths::verify(directory)?;
    Ok(())
}

pub fn shell_setup(arguments: &[String]) -> Option<String> {
    let settings = inline_settings(arguments)?;
    if settings["env"][ENABLED_ENV] != "1" {
        return None;
    }
    let directory = Path::new(settings["env"][DIRECTORY_ENV].as_str()?);
    Some(format!(". {}", quote(directory.join("activate").to_str()?)))
}

pub fn activation(directory: &Path) -> Result<String, String> {
    let directory = quote(supported_path(directory)?);
    Ok(format!(
        "if [ ! -x {directory}/cargo ]; then\n  printf '%s\\n' 'mike fleet: Cargo policy helper is unavailable; refusing launch' >&2\n  return 1\nfi\ncase \":${{PATH-}}:\" in\n  *:{directory}:*) ;;\n  *) export PATH={directory}:\"${{PATH-}}\" ;;\nesac\nhash -r 2>/dev/null || :\n"
    ))
}

fn append_hook(settings: &mut Value, directory: &Path) -> Result<(), String> {
    if settings["disableAllHooks"] == true {
        return Ok(());
    }
    let fields = settings
        .as_object_mut()
        .ok_or("fleet settings must be an object")?;
    let hooks = fields
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("fleet hooks must be an object")?;
    let session = hooks
        .entry("SessionStart")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("fleet SessionStart hooks must be an array")?;
    let fragment = format!(". {}", quote(supported_path(&directory.join("activate"))?));
    let handler = json!({"hooks": [{"type": "command", "command": format!(
        "test -n \"${{CLAUDE_ENV_FILE:-}}\" && printf '\\n%s\\n' {} >> \"$CLAUDE_ENV_FILE\"", quote(&fragment)), "timeout": 5}]});
    if !session.contains(&handler) {
        session.push(handler);
    }
    Ok(())
}

fn inline_settings(arguments: &[String]) -> Option<Value> {
    let mut values = arguments.iter();
    while let Some(value) = values.next() {
        if value == "--settings" {
            return serde_json::from_str(values.next()?).ok();
        }
        if let Some(value) = value.strip_prefix("--settings=") {
            return serde_json::from_str(value).ok();
        }
    }
    None
}

fn create_private(directory: &Path) -> Result<(), String> {
    match std::fs::DirBuilder::new().mode(0o700).create(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Cargo policy private directory could not be created".into()),
    }
    let metadata = directory
        .symlink_metadata()
        .map_err(|_| "Cargo policy directory cannot be inspected")?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(
            "existing Cargo policy directory is not private; permissions were not changed".into(),
        );
    }
    Ok(())
}

fn write_once(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path);
    match file {
        Ok(mut file) => file
            .write_all(bytes)
            .map_err(|_| "Cargo policy file could not be written".into()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
                .open(path)
                .map_err(|_| "existing Cargo policy file cannot be opened safely")?;
            let metadata = file
                .metadata()
                .map_err(|_| "Cargo policy file cannot be inspected")?;
            if !metadata.is_file()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.nlink() != 1
                || metadata.mode() & 0o077 != 0
            {
                return Err(
                    "existing Cargo policy file is not a private unshared regular file".into(),
                );
            }
            let mut actual = Vec::new();
            file.take(bytes.len() as u64 + 1)
                .read_to_end(&mut actual)
                .map_err(|_| "existing Cargo policy file cannot be read")?;
            if actual != bytes {
                return Err("existing Cargo policy file conflicts; nothing was overwritten".into());
            }
            Ok(())
        }
        Err(_) => Err("Cargo policy file could not be created safely".into()),
    }
}

fn supported_path(path: &Path) -> Result<&str, String> {
    let text = path
        .to_str()
        .ok_or("Cargo policy installation requires a UTF-8 path")?;
    if !path.is_absolute() || text.contains(':') || text.chars().any(char::is_control) {
        return Err("Cargo policy installation requires an absolute path without colons or control characters".into());
    }
    Ok(text)
}

pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(feature = "test-seams")]
#[path = "install_cases.rs"]
pub mod cases;
