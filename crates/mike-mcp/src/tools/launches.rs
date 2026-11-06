use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_default, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const LAUNCH_TOOL_NAMES: &[&str] = &[TOOL_SPAWN_SESSION, TOOL_LIST_LAUNCHES, TOOL_KILL_SESSION];

pub const TOOL_SPAWN_SESSION: &str = "spawn_session";
pub const TOOL_LIST_LAUNCHES: &str = "list_launches";
pub const TOOL_KILL_SESSION: &str = "kill_session";

pub fn launch_tools() -> Vec<Tool> {
    vec![
        spawn_session_tool(),
        list_launches_tool(),
        kill_session_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    LAUNCH_TOOL_NAMES.contains(&name)
}

fn spawn_session_surface_properties() -> Vec<(&'static str, Value)> {
    vec![
        (
            "folder",
            json!({
                "type": "string",
                "description": "Folder to open the session in, resolved absolute and \
                    symlink-free by the launcher. Required — matches `mike spawn open \
                    <folder>`."
            }),
        ),
        (
            "claude_args",
            json!({
                "type": "array",
                "items": {"type": "string"},
                "description": "Extra arguments forwarded to `claude` verbatim, in order, \
                    after mike's own flags (e.g. [\"--model\", \"opus\"])."
            }),
        ),
        (
            "prompt",
            json!({
                "type": "string",
                "description": "The first-turn prompt. A short prompt is passed inline; a long \
                    one is pushed to the session over Remote Control once it is up (launch_brief \
                    toggle on; the launch's `brief_read_at` records when it was taken in) or \
                    staged to a private brief file. Either way it is recorded as the session's \
                    `brief` note (list_session_notes kind \"brief\")."
            }),
        ),
        (
            "run",
            json!({
                "type": "string",
                "description": "Run id grouping this launch with sibling launches in one \
                    tab, on a backend that supports tab placement."
            }),
        ),
        (
            "seat",
            json!({
                "type": "string",
                "description": "A short label for this launch within its run (e.g. \"worker-1\"), \
                    recorded on the ledger row. Also the session's identity name when `name` is \
                    omitted."
            }),
        ),
        (
            "worktree_ok",
            json!({
                "type": "boolean",
                "description": "Allow launching inside a linked git worktree, refused by \
                    default."
            }),
        ),
        (
            "trust",
            json!({
                "type": "boolean",
                "description": "Record the folder as trusted in ~/.claude.json before \
                    launch, so Claude Code's own trust dialog cannot swallow the prompt."
            }),
        ),
        (
            "proxy_env",
            json!({
                "type": "boolean",
                "description": "Inject the proxy environment (HTTPS_PROXY / NO_PROXY / \
                    NODE_EXTRA_CA_CERTS) into the surface. Default true."
            }),
        ),
        (
            "remote_control",
            json!({
                "type": "boolean",
                "description": "Append --remote-control so the session is restartable and \
                    pushable. Default true."
            }),
        ),
    ]
}

fn spawn_session_identity_properties() -> Vec<(&'static str, Value)> {
    vec![
        (
            "name",
            json!({
                "type": "string",
                "description": "The name the spawned session registers under, shown to peers on \
                    the roster and in briefings. Defaults to `seat`; pass it only when it should \
                    differ. Applied when the session registers, before its first turn."
            }),
        ),
        (
            "role",
            json!({
                "type": "string",
                "description": "The role the spawned session answers to: what a `role:<role>` \
                    send_session_message resolves against. Applied at registration, before its \
                    first turn."
            }),
        ),
        (
            "instructions",
            json!({
                "type": "string",
                "description": "The standing brief the spawned session works under. \
                    Recorded on the launch ledger and applied automatically at \
                    registration, before the session's first turn."
            }),
        ),
        (
            "goal",
            json!({
                "type": "string",
                "description": "What the spawned session is trying to accomplish, shown on the \
                    roster. Applied at registration, before its first turn; `prompt` carries the \
                    first-turn instructions."
            }),
        ),
    ]
}

fn spawn_session_tool() -> Tool {
    let mut properties = spawn_session_surface_properties();
    properties.extend(spawn_session_identity_properties());
    let schema = object_schema(properties, &["folder"]);
    Tool::new(
        Cow::Borrowed(TOOL_SPAWN_SESSION),
        Cow::Borrowed(
            "Open a terminal surface and start a fresh Claude Code session in it, wired to the \
             proxy with a preassigned session id. Mutating: this opens a real terminal surface \
             and starts a real agent process on the host mike runs on. Pass `name`, `role`, \
             `instructions` and `goal` and the session is on the roster with that identity \
             before its first turn; no register_workspace_session or set_session_goal call is \
             needed. Returns `{launch_id, session, backend}` as soon as the local checks pass \
             (trust gate, worktree refusal, surface cap, ledger row written); opening the \
             surface, registering the session and waiting for its Remote Control channel \
             continue in the background. Poll list_launches for the returned session, or \
             get_workspace_roster once it should have registered. A synchronous refusal (bad \
             folder, untrusted folder without `trust`, no terminal backend configured, surface \
             cap reached) returns an error instead of a launch_id.",
        ),
        schema,
    )
}

fn list_launches_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "run",
            json!({
                "type": "string",
                "description": "Only launches recorded under this run id. Omit for the most \
                    recent launches overall."
            }),
        )],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_LAUNCHES),
        Cow::Borrowed(
            "List the launch attempts recorded by mike's launcher, newest first, as full rows: \
             folder, backend, session id, the full claude argv and brief text, placement, \
             channel-check outcome, and result (pending, success, unconfirmed or failed). \
             Read-only. Use it to watch a spawn_session call progress, or to find a session's \
             surface before kill_session.",
        ),
        schema,
    )
}

fn kill_session_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "session_id",
            json!({
                "type": "string",
                "description": "Session id (a canonical UUID) whose terminal surface should be \
                    closed."
            }),
        )],
        &["session_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_KILL_SESSION),
        Cow::Borrowed(
            "Close the terminal surface hosting `session_id`, waiting briefly for the claude \
             process to exit. Mutating. A session with no known surface, or a surface already \
             gone, returns `closed: false` instead of an error: check `closed` and \
             `process_exited` in the response.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpawnSessionArgs {
    pub(crate) folder: String,
    #[serde(default)]
    pub(crate) claude_args: Vec<String>,
    #[serde(default)]
    pub(crate) prompt: Option<String>,
    #[serde(default)]
    pub(crate) run: Option<String>,
    #[serde(default)]
    pub(crate) seat: Option<String>,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) role: Option<String>,
    #[serde(default)]
    pub(crate) instructions: Option<String>,
    #[serde(default)]
    pub(crate) goal: Option<String>,
    #[serde(default)]
    pub(crate) worktree_ok: bool,
    #[serde(default)]
    pub(crate) trust: bool,
    #[serde(default)]
    pub(crate) proxy_env: Option<bool>,
    #[serde(default)]
    pub(crate) remote_control: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListLaunchesArgs {
    #[serde(default)]
    pub(crate) run: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KillSessionArgs {
    pub(crate) session_id: String,
}

fn spawn_session_body(args: SpawnSessionArgs) -> Value {
    let mut body = Map::new();
    body.insert("folder".to_string(), Value::String(args.folder));
    if !args.claude_args.is_empty() {
        body.insert("claude_args".to_string(), json!(args.claude_args));
    }
    if let Some(prompt) = args.prompt {
        body.insert("prompt".to_string(), Value::String(prompt));
    }
    if let Some(run) = args.run {
        body.insert("run".to_string(), Value::String(run));
    }
    let effective_name = args.name.or_else(|| args.seat.clone());
    if let Some(seat) = args.seat {
        body.insert("seat".to_string(), Value::String(seat));
    }
    if let Some(name) = effective_name {
        body.insert("name".to_string(), Value::String(name));
    }
    if let Some(role) = args.role {
        body.insert("role".to_string(), Value::String(role));
    }
    if let Some(instructions) = args.instructions {
        body.insert("instructions".to_string(), Value::String(instructions));
    }
    if let Some(goal) = args.goal {
        body.insert("goal".to_string(), Value::String(goal));
    }
    if args.worktree_ok {
        body.insert("worktree_ok".to_string(), Value::Bool(true));
    }
    if args.trust {
        body.insert("trust".to_string(), Value::Bool(true));
    }
    if let Some(proxy_env) = args.proxy_env {
        body.insert("proxy_env".to_string(), Value::Bool(proxy_env));
    }
    if let Some(remote_control) = args.remote_control {
        body.insert("remote_control".to_string(), Value::Bool(remote_control));
    }
    let caller = caller_env(|key| std::env::var(key).ok());
    if !caller.is_empty() {
        body.insert("caller_env".to_string(), Value::Object(caller));
    }
    Value::Object(body)
}

const CALLER_ENV_KEYS: [&str; 4] = ["HC_TERMINAL_ID", "HERDR_PANE_ID", "TMUX_PANE", "TMUX"];

fn caller_env(lookup: impl Fn(&str) -> Option<String>) -> Map<String, Value> {
    CALLER_ENV_KEYS
        .iter()
        .filter_map(|key| {
            lookup(key)
                .filter(|v| !v.is_empty())
                .map(|v| (key.to_string(), Value::String(v)))
        })
        .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_SPAWN_SESSION => {
            let args = parse_args_required::<SpawnSessionArgs>(arguments)?;
            let value = client
                .spawn_session(&spawn_session_body(args))
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("launch", value))
        }
        TOOL_LIST_LAUNCHES => {
            let args = parse_args_default::<ListLaunchesArgs>(arguments)?;
            let value = client
                .list_launches(args.run.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("launches", value))
        }
        TOOL_KILL_SESSION => {
            let args = parse_args_required::<KillSessionArgs>(arguments)?;
            let value = client
                .kill_session(&args.session_id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("result", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown launch tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_env_keeps_only_present_terminal_keys() {
        let env = caller_env(|key| match key {
            "HC_TERMINAL_ID" => Some("t13".to_string()),
            "TMUX" => Some(String::new()),
            _ => None,
        });
        assert_eq!(Value::Object(env), json!({"HC_TERMINAL_ID": "t13"}));
    }

    #[test]
    fn caller_env_outside_any_terminal_is_empty() {
        assert!(caller_env(|_| None).is_empty());
    }
}
