use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::sandbox::{INSIDE_BIN, INSIDE_HOME, INSPECT_LISTEN};

const CREDENTIALS_MARGIN_SECONDS: i64 = 600;
const INPUT_NAME_MAX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunDir {
    pub(crate) root: PathBuf,
    pub(crate) input: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) socket: PathBuf,
    pub(crate) stdout: PathBuf,
    pub(crate) stderr: PathBuf,
}

impl RunDir {
    pub(crate) fn new(runs: &Path, id: i64) -> Self {
        let root = runs.join(id.to_string());
        Self {
            input: root.join("in"),
            output: root.join("out"),
            home: root.join("home"),
            socket: root.join("sock").join("relay.sock"),
            stdout: root.join(super::refusal::STDOUT_FILE),
            stderr: root.join("stderr.log"),
            root,
        }
    }

    pub(crate) fn files(&self, output_file: &str) -> Value {
        json!({
            "brief": self.input.join("brief.txt"),
            "inputs": self.input,
            "output": self.output.join(output_file),
            "stdout": self.stdout,
            "stderr": self.stderr,
        })
    }

    pub(crate) fn create(&self) -> Result<(), String> {
        let sock = self.socket.parent().unwrap_or(&self.root).to_path_buf();
        for dir in [&self.root, &self.input, &self.output, &self.home, &sock] {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("cannot restrict {}: {e}", dir.display()))?;
        }
        Ok(())
    }

    pub(crate) fn remove_home(&self) {
        if let Err(e) = std::fs::remove_dir_all(&self.home) {
            tracing::warn!(error = %e, home = %self.home.display(), "wright: the run's home copy could not be removed");
        }
    }
}

pub(crate) fn valid_input_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= INPUT_NAME_MAX
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.'))
        && !name.starts_with('.')
        && !name.contains("..");
    if ok {
        return Ok(());
    }
    Err(format!(
        "input name `{name}` is not allowed; use 1-{INPUT_NAME_MAX} of a-z, 0-9, `_`, `-`, `.`, not starting with `.`"
    ))
}

pub(crate) fn write_inputs(dir: &RunDir, inputs: &[(String, Value)]) -> Result<(), String> {
    for (name, value) in inputs {
        valid_input_name(name)?;
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| format!("input `{name}` cannot be written as JSON: {e}"))?;
        let path = dir.input.join(name);
        std::fs::write(&path, bytes)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}

fn expires_at(oauth: &Value) -> Option<DateTime<Utc>> {
    oauth
        .get("expiresAt")
        .and_then(Value::as_i64)
        .and_then(DateTime::<Utc>::from_timestamp_millis)
}

pub(crate) fn login_expiry(raw: &[u8]) -> Option<DateTime<Utc>> {
    serde_json::from_slice::<Value>(raw)
        .ok()
        .and_then(|parsed| parsed.get("claudeAiOauth").and_then(expires_at))
}

pub(crate) fn login_is_short(raw: &[u8], deadline: DateTime<Utc>) -> bool {
    serde_json::from_slice::<Value>(raw)
        .ok()
        .and_then(|parsed| parsed.get("claudeAiOauth").and_then(expires_at))
        .is_some_and(|expiry| {
            expiry < deadline + chrono::Duration::seconds(CREDENTIALS_MARGIN_SECONDS)
        })
}

pub(crate) fn claude_credentials(raw: &[u8], deadline: DateTime<Utc>) -> Result<Value, String> {
    let parsed: Value = serde_json::from_slice(raw).map_err(|e| {
        format!(
            "the Claude credentials file is not JSON (line {}, column {})",
            e.line(),
            e.column()
        )
    })?;
    let oauth = parsed
        .get("claudeAiOauth")
        .filter(|v| v.is_object())
        .ok_or("the Claude credentials file has no claudeAiOauth login")?;
    if let Some(expiry) = expires_at(oauth) {
        let needed = deadline + chrono::Duration::seconds(CREDENTIALS_MARGIN_SECONDS);
        if expiry < needed {
            return Err(format!(
                "the Claude login expires at {expiry}, before the run's deadline {deadline} plus {CREDENTIALS_MARGIN_SECONDS} s; Wright asked Claude Code to renew it and it stayed short; run `claude` on this machine and log in"
            ));
        }
    }
    Ok(json!({ "claudeAiOauth": oauth }))
}

pub(crate) fn settings(tools: &[&str]) -> Value {
    let mut allow: Vec<String> = tools.iter().map(|t| (*t).to_owned()).collect();
    allow.push("mcp__mike".to_owned());
    json!({
        "disableAllHooks": true,
        "autoUpdates": false,
        "includeCoAuthoredBy": false,
        "permissions": { "allow": allow, "defaultMode": "dontAsk" },
    })
}

pub(crate) fn mcp_config() -> Value {
    json!({
        "mcpServers": {
            "mike": {
                "type": "stdio",
                "command": format!("{INSIDE_BIN}/mike-mcp"),
                "args": [],
                "env": { "MIKE_INSPECT_URL": format!("http://{INSPECT_LISTEN}") },
            }
        }
    })
}

fn write_private(path: &Path, value: &Value) -> Result<PathBuf, String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("cannot restrict {}: {e}", path.display()))?;
    Ok(path.to_path_buf())
}

pub(crate) struct HomeInputs<'a> {
    pub(crate) credentials: &'a Path,
    pub(crate) deadline: DateTime<Utc>,
    pub(crate) tools: &'a [&'a str],
}

pub(crate) fn write_home(
    dir: &RunDir,
    inputs: &HomeInputs<'_>,
) -> Result<Vec<(PathBuf, &'static str)>, String> {
    let raw = std::fs::read(inputs.credentials).map_err(|e| {
        format!(
            "cannot read the Claude credentials at {}: {e}",
            inputs.credentials.display()
        )
    })?;
    let credentials = claude_credentials(&raw, inputs.deadline)?;
    Ok(vec![
        (
            write_private(&dir.home.join("credentials.json"), &credentials)?,
            ".claude/.credentials.json",
        ),
        (
            write_private(&dir.home.join("settings.json"), &settings(inputs.tools))?,
            ".claude/settings.json",
        ),
        (
            write_private(&dir.home.join("mcp.json"), &mcp_config())?,
            "mcp.json",
        ),
    ])
}

const INSTRUCTION_DIRS: [(&str, &str); 3] = [
    (".claude/agents", ".claude/agents"),
    (".claude/rules", ".claude/rules"),
    (".claude/skills", ".claude/skills"),
];

pub(crate) fn instruction_dirs(repo: &Path) -> Vec<(PathBuf, &'static str)> {
    INSTRUCTION_DIRS
        .iter()
        .map(|(from, to)| (repo.join(from), *to))
        .filter(|(path, _)| path.is_dir())
        .collect()
}

pub(crate) fn inside_mcp_config() -> String {
    format!("{INSIDE_HOME}/mcp.json")
}
