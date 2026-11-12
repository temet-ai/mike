use std::path::PathBuf;

use serde_json::{Map, Value, json};

use super::ThreadPolicy;
use super::rpc::ServerCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpBinding {
    pub name: String,
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
        json!({self.name.clone(): {"command": self.command, "env": env}})
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
    pub policy: ThreadPolicy,
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
        params.insert("sandbox".into(), json!(self.policy.sandbox));
        params.insert("approvalPolicy".into(), json!(self.policy.approval));
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
