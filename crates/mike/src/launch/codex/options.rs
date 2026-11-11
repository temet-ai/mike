use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::CodexError;
use super::rpc::ServerCommand;

const SANDBOX: &str = "workspace-write";
const APPROVAL: &str = "never";
const AUTH_FILE: &str = "auth.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpBinding {
    pub command: String,
    pub env: Vec<(String, String)>,
}

impl McpBinding {
    fn config(&self) -> Value {
        let env: Map<String, Value> = self
            .env
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        json!({"mike": {"command": self.command, "env": env}})
    }
}

#[derive(Debug, Clone)]
pub struct SeatOptions {
    pub program: PathBuf,
    pub cwd: PathBuf,
    pub codex_home: PathBuf,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub developer_instructions: String,
    pub mcp: Option<McpBinding>,
}

impl SeatOptions {
    pub(crate) fn server(&self) -> ServerCommand {
        ServerCommand {
            program: self.program.clone(),
            cwd: self.cwd.clone(),
            codex_home: self.codex_home.clone(),
            env: Vec::new(),
        }
    }

    fn shared(&self) -> Map<String, Value> {
        let mut params = Map::new();
        params.insert("cwd".into(), json!(self.cwd.to_string_lossy()));
        params.insert("sandbox".into(), json!(SANDBOX));
        params.insert("approvalPolicy".into(), json!(APPROVAL));
        params.insert(
            "developerInstructions".into(),
            json!(self.developer_instructions),
        );
        if let Some(model) = self.model.as_deref().filter(|m| !m.trim().is_empty()) {
            params.insert("model".into(), json!(model));
        }
        if let Some(mcp) = &self.mcp {
            params.insert("config".into(), json!({"mcp_servers": mcp.config()}));
        }
        params
    }

    pub(crate) fn thread_start(&self) -> Value {
        let mut params = self.shared();
        params.insert("ephemeral".into(), json!(false));
        Value::Object(params)
    }

    pub(crate) fn thread_resume(&self, thread_id: &str) -> Value {
        let mut params = self.shared();
        params.insert("threadId".into(), json!(thread_id));
        params.insert("excludeTurns".into(), json!(true));
        Value::Object(params)
    }

    pub(crate) fn thread_fork(&self, thread_id: &str) -> Value {
        let mut params = self.shared();
        params.insert("threadId".into(), json!(thread_id));
        params.insert("ephemeral".into(), json!(false));
        params.insert("excludeTurns".into(), json!(true));
        Value::Object(params)
    }

    pub(crate) fn turn_start(&self, thread_id: &str, text: &str) -> Value {
        let mut params = json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": text}],
        });
        if let Some(effort) = self.effort.as_deref().filter(|e| !e.trim().is_empty()) {
            params["effort"] = json!(effort);
        }
        params
    }
}

pub fn seat_codex_home(scratch: &Path, run_id: &str) -> PathBuf {
    let run: String = run_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    scratch.join("codex").join(run)
}

pub(crate) fn prepare_home(home: &Path, operator_home: Option<&Path>) -> Result<(), CodexError> {
    let failed = |error: std::io::Error| CodexError::Home {
        path: home.display().to_string(),
        error: error.to_string(),
    };
    std::fs::create_dir_all(home).map_err(failed)?;
    let Some(source) = operator_home
        .map(|h| h.join(AUTH_FILE))
        .filter(|p| p.exists())
    else {
        return Ok(());
    };
    let link = home.join(AUTH_FILE);
    if link.symlink_metadata().is_ok() {
        return Ok(());
    }
    std::os::unix::fs::symlink(source, link).map_err(failed)
}

pub(crate) fn operator_codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")))
}
