use std::time::Duration;

use uuid::Uuid;

use super::cli::{DEFAULT_TIMEOUT, TmuxCli, argv};
use crate::launch::backend::{BackendId, Surface};
use crate::launch::error::LaunchError;

pub(super) use crate::launch::TMUX_SESSION_PREFIX as SESSION_PREFIX;

pub(super) fn session_name(session: Uuid) -> String {
    format!("{SESSION_PREFIX}{}", &session.simple().to_string()[..8])
}

pub(super) fn session_name_from_agent(name: &str) -> String {
    format!(
        "{SESSION_PREFIX}{}",
        name.strip_prefix("cc-").unwrap_or(name)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PaneInfo {
    pub(super) session: String,
    pub(super) pane_id: String,
    pub(super) window_index: String,
    pub(super) cwd: String,
    pub(super) current_command: String,
    pub(super) session_uuid: Option<Uuid>,
}

const LIST_PANES_FORMAT: &str = "#{session_name}\x1f#{pane_id}\x1f#{window_index}\x1f#{pane_current_path}\x1f#{pane_current_command}\x1f#{@mike_session}";

fn parse_pane_line(line: &str) -> Option<PaneInfo> {
    let mut fields = line.split('\x1f');
    let session = fields.next()?.to_string();
    let pane_id = fields.next()?.to_string();
    let window_index = fields.next()?.to_string();
    let cwd = fields.next()?.to_string();
    let current_command = fields.next()?.to_string();
    let session_uuid = fields.next().and_then(|v| v.parse().ok());
    Some(PaneInfo {
        session,
        pane_id,
        window_index,
        cwd,
        current_command,
        session_uuid,
    })
}

pub(super) async fn all_mike_panes(cli: &TmuxCli) -> Result<Vec<PaneInfo>, LaunchError> {
    let args = argv(&["list-panes", "-a", "-F", LIST_PANES_FORMAT]);
    let text = match cli.run_text(&args, DEFAULT_TIMEOUT).await {
        Ok(text) => text,
        Err(LaunchError::Failed(detail)) if detail.contains("no server running") => String::new(),
        Err(e) => return Err(e),
    };
    Ok(text
        .lines()
        .filter_map(parse_pane_line)
        .filter(|p| p.session.starts_with(SESSION_PREFIX))
        .collect())
}

pub(super) fn surface_from_pane(
    pane: &PaneInfo,
    pid: Option<u32>,
    placement: &str,
    reused: bool,
) -> Surface {
    Surface {
        backend: BackendId::Tmux,
        workspace_ref: Some(pane.session.clone()),
        tab_ref: Some(format!("{}:{}", pane.session, pane.window_index)),
        surface_ref: pane.pane_id.clone(),
        pid,
        session_uuid: pane.session_uuid,
        agent_state: None,
        cwd: Some(pane.cwd.clone()),
        actual_placement: placement.to_string(),
        reused,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    }
}

pub(super) async fn pane_exists(cli: &TmuxCli, pane: &str) -> Result<bool, LaunchError> {
    cli.run_missing_ok(&argv(&["list-panes", "-t", pane]), DEFAULT_TIMEOUT)
        .await
}

pub(super) async fn pane_current_command(cli: &TmuxCli, pane: &str) -> Option<String> {
    let args = argv(&[
        "display-message",
        "-p",
        "-t",
        pane,
        "-F",
        "#{pane_current_command}",
    ]);
    let text = cli.run_text(&args, DEFAULT_TIMEOUT).await.ok()?;
    Some(text.trim().to_string())
}

pub(super) async fn foreground_child_pid(cli: &TmuxCli, pane: &str) -> Option<u32> {
    let args = argv(&["display-message", "-p", "-t", pane, "-F", "#{pane_pid}"]);
    let text = cli.run_text(&args, DEFAULT_TIMEOUT).await.ok()?;
    let shell_pid: u32 = text.trim().parse().ok()?;
    crate::launch::process::claude_child_of(shell_pid)
}

pub(super) async fn wait_for_exit(pid: u32, wait: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if !crate::launch::process::alive(pid) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        let step = Duration::from_millis(200);
        tokio::time::sleep(
            step.min(deadline.saturating_duration_since(tokio::time::Instant::now())),
        )
        .await;
    }
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
    fn session_name_matches_agent_name_derivation_for_the_same_uuid() {
        let id = Uuid::parse_str("d34dbeef-0000-0000-0000-000000000000").unwrap();
        let agent = crate::launch::request::agent_name(id);
        assert_eq!(session_name(id), session_name_from_agent(&agent));
        assert_eq!(session_name(id), "mike-d34dbeef");
    }

    #[test]
    fn parse_pane_line_reads_every_field_and_empty_session_uuid_is_none() {
        let line = "mike-abcd1234\x1f%3\x1f0\x1f/tmp/proj\x1fclaude\x1f";
        let pane = parse_pane_line(line).unwrap();
        assert_eq!(pane.session, "mike-abcd1234");
        assert_eq!(pane.pane_id, "%3");
        assert_eq!(pane.current_command, "claude");
        assert_eq!(pane.session_uuid, None);
    }

    #[test]
    fn parse_pane_line_with_a_recorded_uuid() {
        let id = Uuid::new_v4();
        let line = format!("mike-x\x1f%1\x1f0\x1f/tmp\x1fzsh\x1f{id}");
        let pane = parse_pane_line(&line).unwrap();
        assert_eq!(pane.session_uuid, Some(id));
    }

    #[test]
    fn parse_pane_line_too_few_fields_is_none() {
        assert!(parse_pane_line("only\x1ftwo").is_none());
    }

    #[tokio::test]
    async fn wait_for_exit_gone_pid_is_true_live_pid_is_false() {
        assert!(wait_for_exit(u32::MAX, Duration::from_millis(10)).await);
        assert!(!wait_for_exit(std::process::id(), Duration::from_millis(50)).await);
    }
}
