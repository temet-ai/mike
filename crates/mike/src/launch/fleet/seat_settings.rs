use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::definition::{FleetDefinition, merge};
use super::operation::Execution;
use super::{browser_guard, bundle, timed_command};

#[path = "background_hook.rs"]
pub(super) mod background_hook;
#[path = "receipt_hook.rs"]
pub(super) mod receipt_hook;

const SHIMS: &str = "bin";
const ACTIVATE: &str = "activate";
const TIMINGS: &str = "timings";
pub(crate) const PROFILE_ENV: &str = "MIKE_MCP_PROFILE";
const DELETED_SUFFIX: &str = " (deleted)";
const KINDS_WITH_OWN_PROFILE: [&str; 4] = ["coordinator", "acceptance", "observer", "reference"];

pub(crate) fn profile_for(seat: &str) -> &'static str {
    KINDS_WITH_OWN_PROFILE
        .into_iter()
        .find(|kind| {
            *kind == seat
                || seat.strip_prefix(*kind).is_some_and(|rest| {
                    rest.strip_prefix('-').is_some_and(|suffix| {
                        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
                    })
                })
        })
        .unwrap_or("worker")
}

pub(super) fn scratch_dir(state: &Path, repo: &str) -> PathBuf {
    let base = state.parent().unwrap_or(state);
    let name: String = Path::new(repo)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    base.join("seats").join(name)
}

pub(super) fn effective(def: &FleetDefinition, scratch: &Path, seat: &str) -> Value {
    let tmp = scratch.join("tmp").to_string_lossy().into_owned();
    let mut settings = json!({
        "env": {
            "TMPDIR": tmp,
            "BROWSER": "true",
            browser_guard::ENABLED_ENV: "1",
            "CLAUDE_CODE_TMPDIR": tmp,
            "XDG_CONFIG_HOME": scratch.join("config").to_string_lossy(),
            PROFILE_ENV: profile_for(seat),
        },
    });
    if let Some(home) = operator_config_home() {
        settings["env"][crate::config::paths::CONFIG_HOME_ENV] =
            Value::String(home.to_string_lossy().into_owned());
    }
    declared_defaults(&mut settings, &def.execution, scratch);
    if let Some(operator) = def.seat_settings.as_ref().filter(|s| !s.is_null()) {
        merge(&mut settings, operator);
    }
    browser_guard::configure(&mut settings, scratch);
    if !def.execution.timed_commands.is_empty() {
        append_session_start(&mut settings, &scratch.join(SHIMS).join(ACTIVATE));
    }
    if let Ok(executable) = seat_executable() {
        background_hook::append_hook(&mut settings, &executable);
        receipt_hook::append_hook(&mut settings, &executable);
    }
    settings
}

fn operator_config_home() -> Option<PathBuf> {
    let absolute = |name: &str| {
        crate::config::env_text(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    absolute(crate::config::paths::CONFIG_HOME_ENV)
        .or_else(|| absolute("XDG_CONFIG_HOME"))
        .or_else(|| absolute("HOME").map(|home| home.join(".config")))
}

fn declared_defaults(settings: &mut Value, execution: &Execution, scratch: &Path) {
    if !execution.disabled_plugins.is_empty() {
        settings["enabledPlugins"] = execution
            .disabled_plugins
            .iter()
            .map(|id| (id.clone(), json!(false)))
            .collect::<serde_json::Map<String, Value>>()
            .into();
    }
    if !execution.timed_commands.is_empty() {
        merge(
            settings,
            &json!({"env": {
                timed_command::SHIMS_ENV: scratch.join(SHIMS).to_string_lossy(),
                timed_command::DECLARED_ENV: json!(execution.timed_commands).to_string(),
                super::build_timing::DIR_ENV: scratch.join(TIMINGS).to_string_lossy(),
            }}),
        );
    }
    if !execution.background_commands.is_empty() {
        merge(
            settings,
            &json!({"env": {
                background_hook::COMMANDS_ENV: json!(execution.background_commands).to_string(),
            }}),
        );
    }
}

pub(crate) fn for_seat(def: &FleetDefinition, seat: &str) -> Value {
    let state = bundle::default_state_dir(&bundle::home());
    effective(def, &scratch_dir(&state, &def.repo), seat)
}

pub(super) fn timings_dir(def: &FleetDefinition) -> PathBuf {
    let state = bundle::default_state_dir(&bundle::home());
    scratch_dir(&state, &def.repo).join(TIMINGS)
}

pub(crate) fn prepare(def: &FleetDefinition) -> Result<(), String> {
    let state = bundle::default_state_dir(&bundle::home());
    let scratch = scratch_dir(&state, &def.repo);
    create_folders(&scratch);
    let programs: Vec<&str> = def
        .execution
        .timed_commands
        .keys()
        .map(String::as_str)
        .collect();
    if !programs.is_empty() {
        let written = seat_executable().and_then(|exe| write_shims(&scratch, &exe, &programs));
        if let Err(err) = written {
            tracing::warn!(scratch = %scratch.display(), error = %err, "mike could not write the seats' timed command shims");
        }
    }
    browser_guard::prepare(&effective(def, &scratch, "worker"))
}

fn create_folders(scratch: &Path) {
    for folder in [scratch.join("tmp"), scratch.join("config")] {
        if let Err(err) = std::fs::create_dir_all(&folder) {
            tracing::warn!(folder = %folder.display(), error = %err, "mike could not create a seat's scratch folder");
        }
    }
}

fn append_session_start(settings: &mut Value, activate: &Path) {
    if settings["disableAllHooks"] == true {
        return;
    }
    let Some(start) = settings
        .as_object_mut()
        .and_then(|fields| {
            fields
                .entry("hooks")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        })
        .and_then(|hooks| {
            hooks
                .entry("SessionStart")
                .or_insert_with(|| json!([]))
                .as_array_mut()
        })
    else {
        return;
    };
    let fragment = format!("[ -r {0} ] && . {0}", quoted(activate));
    let handler = json!({
        "hooks": [{
            "type": "command",
            "command": format!(
                "test -n \"${{CLAUDE_ENV_FILE:-}}\" && printf '\\n%s\\n' {} >> \"$CLAUDE_ENV_FILE\"",
                quoted(Path::new(&fragment))
            ),
            "timeout": 5,
        }],
    });
    if !start.contains(&handler) {
        start.push(handler);
    }
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

pub(super) fn seat_executable() -> std::io::Result<PathBuf> {
    stable_executable(&std::env::current_exe()?)
}

fn stable_executable(running: &Path) -> std::io::Result<PathBuf> {
    let text = running.to_string_lossy();
    let path = PathBuf::from(text.strip_suffix(DELETED_SUFFIX).unwrap_or(&text));
    if path.is_file() {
        Ok(path)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no mike binary at {}", path.display()),
        ))
    }
}

pub(super) fn write_shims(scratch: &Path, exe: &Path, programs: &[&str]) -> std::io::Result<()> {
    let exe = stable_executable(exe)?;
    let shims = scratch.join(SHIMS);
    std::fs::create_dir_all(&shims)?;
    for program in programs {
        let shim = shims.join(program);
        std::fs::write(
            &shim,
            format!(
                "#!/bin/sh\nexec {} fleet timed-run {} \"$@\"\n",
                quoted(&exe),
                quoted(Path::new(program))
            ),
        )?;
        executable(&shim)?;
    }
    let dir = quoted(&shims);
    std::fs::write(
        shims.join(ACTIVATE),
        format!("case \":$PATH:\" in *:{dir}:*) ;; *) PATH={dir}:\"$PATH\"; export PATH ;; esac\n"),
    )
}

#[cfg(unix)]
fn executable(file: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn executable(_: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "seat_settings_env_tests.rs"]
mod env_tests;
#[cfg(test)]
#[path = "seat_settings_tests.rs"]
mod tests;
