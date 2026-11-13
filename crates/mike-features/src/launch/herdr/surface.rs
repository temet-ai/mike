use serde_json::Value;
use uuid::Uuid;

use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv, str_field};
use crate::launch::backend::{BackendId, Surface};
use crate::launch::error::LaunchError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRef {
    pub workspace: String,
    pub tab: String,
    pub pane: String,
}

impl PaneRef {
    pub fn from_value(value: &Value) -> Option<Self> {
        Some(Self {
            workspace: str_field(value, "workspace_id")?.to_string(),
            tab: str_field(value, "tab_id")?.to_string(),
            pane: str_field(value, "pane_id")?.to_string(),
        })
    }
}

pub fn is_claude_agent(agent: &Value) -> bool {
    str_field(agent, "agent") == Some("claude")
}

pub fn agent_session_uuid(agent: &Value) -> Option<Uuid> {
    let session = agent.get("agent_session")?;
    if str_field(session, "kind").is_some_and(|k| k != "id") {
        return None;
    }
    str_field(session, "value")?.to_lowercase().parse().ok()
}

pub fn surface_from_agent(
    agent: &Value,
    pid: Option<u32>,
    placement: &str,
    reused: bool,
) -> Option<Surface> {
    let pane = PaneRef::from_value(agent)?;
    Some(Surface {
        backend: BackendId::Herdr,
        workspace_ref: Some(pane.workspace),
        tab_ref: Some(pane.tab),
        surface_ref: pane.pane,
        pid,
        session_uuid: agent_session_uuid(agent),
        agent_state: str_field(agent, "agent_status").map(str::to_string),
        cwd: str_field(agent, "cwd").map(str::to_string),
        actual_placement: placement.to_string(),
        reused,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    })
}

pub async fn claude_agents(cli: &HerdrCli) -> Result<Vec<Value>, LaunchError> {
    let result = cli
        .run_json(&argv(&["agent", "list"]), DEFAULT_TIMEOUT)
        .await?;
    let agents = result
        .get("agents")
        .and_then(Value::as_array)
        .ok_or_else(|| LaunchError::Malformed {
            command: "herdr agent list".into(),
            detail: format!("no `agents` array in {result}"),
        })?;
    Ok(agents
        .iter()
        .filter(|a| is_claude_agent(a))
        .cloned()
        .collect())
}

pub async fn find_by_session(
    cli: &HerdrCli,
    session: Uuid,
) -> Result<Option<Surface>, LaunchError> {
    let agents = claude_agents(cli).await?;
    let Some(mut surface) = agents
        .iter()
        .find(|a| agent_session_uuid(a) == Some(session))
        .and_then(|a| surface_from_agent(a, None, "existing", true))
    else {
        return Ok(None);
    };
    surface.pid = pane_pid(cli, &surface.surface_ref).await;
    let still_matching = claude_agents(cli).await?.iter().any(|agent| {
        agent_session_uuid(agent) == Some(session)
            && str_field(agent, "pane_id") == Some(surface.surface_ref.as_str())
    });
    Ok(still_matching.then_some(surface))
}

pub async fn list(cli: &HerdrCli) -> Result<Vec<Surface>, LaunchError> {
    let agents = claude_agents(cli).await?;
    Ok(agents
        .iter()
        .filter_map(|a| surface_from_agent(a, None, "existing", true))
        .collect())
}

pub async fn pane_pid(cli: &HerdrCli, pane: &str) -> Option<u32> {
    let result = cli
        .run_json(
            &argv(&["pane", "process-info", "--pane", pane]),
            DEFAULT_TIMEOUT,
        )
        .await
        .ok()?;
    let info = result.get("process_info")?;
    let processes = info.get("foreground_processes").and_then(Value::as_array);
    let named = processes.and_then(|ps| {
        ps.iter()
            .find(|p| str_field(p, "name") == Some("claude"))
            .and_then(|p| p.get("pid").and_then(Value::as_u64))
    });
    if let Some(pid) = named {
        return u32::try_from(pid).ok();
    }
    let leader = info
        .get("foreground_process_group_id")
        .and_then(Value::as_u64)?;
    let shell = info.get("shell_pid").and_then(Value::as_u64);
    (Some(leader) != shell)
        .then(|| u32::try_from(leader).ok())
        .flatten()
}

pub async fn pane_at_shell_prompt(cli: &HerdrCli, pane: &str) -> Result<bool, LaunchError> {
    let result = cli
        .run_json(
            &argv(&["pane", "process-info", "--pane", pane]),
            DEFAULT_TIMEOUT,
        )
        .await?;
    let info = result
        .get("process_info")
        .ok_or_else(|| LaunchError::Malformed {
            command: "herdr pane process-info".into(),
            detail: format!("no `process_info` in {result}"),
        })?;
    let leader = info
        .get("foreground_process_group_id")
        .and_then(Value::as_u64);
    let shell = info.get("shell_pid").and_then(Value::as_u64);
    Ok(leader.is_some() && leader == shell)
}

pub async fn pane_exists(cli: &HerdrCli, pane: &str) -> Result<bool, LaunchError> {
    match cli
        .run_json(&argv(&["pane", "get", pane]), DEFAULT_TIMEOUT)
        .await
    {
        Ok(_) => Ok(true),
        Err(err) if err.backend_code() == Some("pane_not_found") => Ok(false),
        Err(err) => Err(err),
    }
}
