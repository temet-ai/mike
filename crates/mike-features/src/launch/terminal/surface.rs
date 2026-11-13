use std::time::{Duration, Instant};

use serde_json::Value;
use uuid::Uuid;

use super::cli::TerminalCli;
use crate::launch::backend::{BackendId, Surface};
use crate::launch::error::LaunchError;

pub const APP_ID_PREFIX: &str = "mike-";

pub fn app_id_for_session(session: Uuid) -> String {
    format!("{APP_ID_PREFIX}{session}")
}

pub fn tag_for(session: Option<Uuid>, agent_name: &str) -> String {
    session.map_or_else(
        || format!("{APP_ID_PREFIX}{agent_name}"),
        app_id_for_session,
    )
}

pub struct ClientInfo {
    pub pid: u32,
    pub class: String,
}

fn parse_clients(value: &Value) -> Vec<ClientInfo> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some(ClientInfo {
                pid: u32::try_from(c.get("pid")?.as_i64()?).ok()?,
                class: c.get("class")?.as_str()?.to_string(),
            })
        })
        .collect()
}

pub async fn clients(cli: &TerminalCli, timeout: Duration) -> Result<Vec<ClientInfo>, LaunchError> {
    let value = cli
        .run_hypr_json(&super::cli::argv(&["clients", "-j"]), timeout)
        .await?;
    Ok(parse_clients(&value))
}

pub async fn find_by_pid(
    cli: &TerminalCli,
    pid: u32,
    timeout: Duration,
) -> Result<Option<ClientInfo>, LaunchError> {
    Ok(clients(cli, timeout)
        .await?
        .into_iter()
        .find(|c| c.pid == pid))
}

pub async fn find_by_session(
    cli: &TerminalCli,
    session: Uuid,
    timeout: Duration,
) -> Result<Option<Surface>, LaunchError> {
    let tag = app_id_for_session(session);
    let entry = clients(cli, timeout)
        .await?
        .into_iter()
        .find(|c| c.class == tag);
    Ok(entry.map(|c| {
        let claude_pid = claude_child_of(c.pid);
        surface_from_client(&c, claude_pid, "existing", true)
    }))
}

pub async fn list(cli: &TerminalCli, timeout: Duration) -> Result<Vec<Surface>, LaunchError> {
    Ok(clients(cli, timeout)
        .await?
        .into_iter()
        .filter(|c| c.class.starts_with(APP_ID_PREFIX))
        .map(|c| {
            let claude_pid = claude_child_of(c.pid);
            surface_from_client(&c, claude_pid, "existing", true)
        })
        .collect())
}

pub fn surface_from_client(
    entry: &ClientInfo,
    claude_pid: Option<u32>,
    placement: &str,
    reused: bool,
) -> Surface {
    Surface {
        backend: BackendId::Terminal,
        workspace_ref: None,
        tab_ref: None,
        surface_ref: entry.pid.to_string(),
        pid: claude_pid,
        session_uuid: entry
            .class
            .strip_prefix(APP_ID_PREFIX)
            .and_then(|s| s.parse().ok()),
        agent_state: None,
        cwd: proc_cwd(entry.pid),
        actual_placement: placement.to_string(),
        reused,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    }
}

fn proc_cwd(pid: u32) -> Option<String> {
    crate::launch::process::cwd(pid).map(|p| p.to_string_lossy().into_owned())
}

pub fn claude_child_of(window_pid: u32) -> Option<u32> {
    crate::launch::process::claude_child_of(window_pid)
}

pub async fn wait_for_child_pid(window_pid: u32, budget: Duration) -> Option<u32> {
    let deadline = Instant::now() + budget;
    loop {
        if let Some(pid) = claude_child_of(window_pid) {
            return Some(pid);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

pub async fn wait_for_exit(pid: u32, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        if !crate::launch::process::alive(pid) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        let step = Duration::from_millis(200);
        tokio::time::sleep(step.min(deadline.saturating_duration_since(Instant::now()))).await;
    }
}

pub fn terminate(pid: u32) -> Result<(), LaunchError> {
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if rc == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(LaunchError::io(
        format!("sending SIGTERM to pid {pid}"),
        err,
    ))
}

pub fn spawn_reaper(mut child: tokio::process::Child) {
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use super::*;

    #[test]
    fn tag_for_uses_the_session_uuid_when_known_else_the_agent_name() {
        let id = Uuid::new_v4();
        assert_eq!(tag_for(Some(id), "cc-x"), app_id_for_session(id));
        assert_eq!(tag_for(None, "cc-abcd1234"), "mike-cc-abcd1234");
    }

    #[test]
    fn parse_clients_reads_pid_and_class_and_skips_incomplete_rows() {
        let value = serde_json::json!([
            {"pid": 123, "class": "mike-abc"},
            {"pid": 456},
            {"class": "no-pid"},
        ]);
        let clients = parse_clients(&value);
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].pid, 123);
        assert_eq!(clients[0].class, "mike-abc");
    }

    #[test]
    fn surface_from_client_parses_a_full_session_uuid_out_of_the_tag() {
        let id = Uuid::new_v4();
        let entry = ClientInfo {
            pid: 42,
            class: app_id_for_session(id),
        };
        let surface = surface_from_client(&entry, Some(99), "existing", true);
        assert_eq!(surface.session_uuid, Some(id));
        assert_eq!(surface.surface_ref, "42");
        assert_eq!(surface.pid, Some(99));
    }

    #[test]
    fn surface_from_client_with_an_agent_name_tag_has_no_session_uuid() {
        let entry = ClientInfo {
            pid: 42,
            class: "mike-cc-abcd1234".to_string(),
        };
        let surface = surface_from_client(&entry, None, "existing", true);
        assert_eq!(surface.session_uuid, None);
    }

    #[tokio::test]
    async fn wait_for_exit_gone_pid_is_true_live_pid_is_false() {
        assert!(wait_for_exit(u32::MAX, Duration::from_millis(10)).await);
        assert!(!wait_for_exit(std::process::id(), Duration::from_millis(50)).await);
    }

    #[test]
    fn terminate_an_already_gone_pid_is_ok_not_an_error() {
        assert!(terminate(999_999_999).is_ok());
    }

    #[tokio::test]
    async fn wait_for_child_pid_no_children_file_times_out_to_none() {
        assert_eq!(
            wait_for_child_pid(u32::MAX, Duration::from_millis(50)).await,
            None
        );
    }
}
