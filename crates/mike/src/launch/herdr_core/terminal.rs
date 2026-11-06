use serde_json::Value;
use uuid::Uuid;

use super::cli::{DEFAULT_TIMEOUT, HcCli, argv, str_field};
use crate::launch::backend::{BackendId, Surface};
use crate::launch::error::LaunchError;

pub(super) const SPACE_LABEL: &str = "herdr.space";
pub(super) const TAB_LABEL: &str = "herdr.tab";
pub(super) const SESSION_LABEL: &str = "mike.session";
pub(super) const NAME_LABEL: &str = "mike.name";
pub(super) const RUN_LABEL: &str = "mike.run";
pub(super) const SEAT_LABEL: &str = "mike.seat";
pub(super) const LAUNCHED_LABEL: &str = "mike.launched";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Terminal {
    pub(super) address: String,
    pub(super) live: bool,
    pub(super) cwd: Option<String>,
    pub(super) pid: Option<u32>,
    pub(super) status: Option<String>,
    pub(super) turn: Option<u64>,
    pub(super) space: Option<String>,
    pub(super) tab: Option<String>,
    pub(super) session: Option<Uuid>,
}

impl Terminal {
    pub(super) fn from_get(value: &Value) -> Option<Self> {
        let labels = value.get("labels").unwrap_or(&Value::Null);
        Some(Self {
            address: str_field(value, "address")?.to_string(),
            live: str_field(value, "state") == Some("live"),
            cwd: str_field(value, "cwd").map(str::to_string),
            pid: foreground_pid(value),
            status: value
                .pointer("/status/state")
                .and_then(Value::as_str)
                .map(str::to_string),
            turn: value.get("turn").and_then(Value::as_u64),
            space: str_field(labels, SPACE_LABEL).map(str::to_string),
            tab: str_field(labels, TAB_LABEL).map(str::to_string),
            session: str_field(labels, SESSION_LABEL).and_then(|s| s.parse().ok()),
        })
    }

    pub(super) fn surface(&self, placement: &str, reused: bool) -> Surface {
        Surface {
            backend: BackendId::HerdrCore,
            workspace_ref: self.space.clone(),
            tab_ref: self.tab.clone(),
            surface_ref: self.address.clone(),
            pid: self.pid,
            session_uuid: self.session,
            agent_state: self.status.clone(),
            cwd: self.cwd.clone(),
            actual_placement: placement.to_string(),
            reused,
            delivered: None,
            confirmed: false,
            prompt_detail: None,
        }
    }
}

fn foreground_pid(value: &Value) -> Option<u32> {
    let pid = value
        .pointer("/foreground/pid")
        .or_else(|| value.get("pid"))
        .and_then(Value::as_u64)?;
    u32::try_from(pid).ok()
}

pub(super) async fn get(cli: &HcCli, address: &str) -> Result<Option<Terminal>, LaunchError> {
    match cli
        .hc_json(&argv(&["get", address, "--json"]), DEFAULT_TIMEOUT)
        .await
    {
        Ok(value) => Ok(Terminal::from_get(&value)),
        Err(err) if err.backend_code() == Some("not_found") => Ok(None),
        Err(err) => Err(err),
    }
}

pub(super) async fn addresses_labelled(
    cli: &HcCli,
    key: &str,
    value: &str,
) -> Result<Vec<String>, LaunchError> {
    let label = format!("{key}={value}");
    let listed = cli
        .hc_json(&argv(&["ls", "--json", "--label", &label]), DEFAULT_TIMEOUT)
        .await?;
    Ok(live_addresses(&listed))
}

pub(super) fn live_addresses(listed: &Value) -> Vec<String> {
    listed
        .get("terminals")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|t| str_field(t, "state").is_none_or(|s| s == "live"))
        .filter_map(|t| str_field(t, "address").map(str::to_string))
        .collect()
}

pub(super) async fn labelled(
    cli: &HcCli,
    key: &str,
    value: &str,
) -> Result<Vec<Terminal>, LaunchError> {
    let mut found = Vec::new();
    for address in addresses_labelled(cli, key, value).await? {
        if let Some(terminal) = get(cli, &address).await? {
            found.push(terminal);
        }
    }
    Ok(found)
}

pub(super) async fn find_by_session(
    cli: &HcCli,
    session: Uuid,
) -> Result<Option<Surface>, LaunchError> {
    let terminals = labelled(cli, SESSION_LABEL, &session.to_string()).await?;
    Ok(terminals
        .into_iter()
        .find(|t| t.live && t.session == Some(session))
        .map(|t| t.surface("existing", true)))
}

pub(super) async fn list(cli: &HcCli) -> Result<Vec<Surface>, LaunchError> {
    let terminals = labelled(cli, LAUNCHED_LABEL, "1").await?;
    Ok(terminals
        .iter()
        .filter(|t| t.live)
        .map(|t| t.surface("existing", true))
        .collect())
}

pub(super) async fn by_name(cli: &HcCli, name: &str) -> Result<Option<Terminal>, LaunchError> {
    let terminals = labelled(cli, NAME_LABEL, name).await?;
    Ok(terminals.into_iter().find(|t| t.live))
}
