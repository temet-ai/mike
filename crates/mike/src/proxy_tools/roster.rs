use serde_json::Value;

use crate::config::AppState;
use crate::workspace::RosterScope;

pub(crate) const TOOL_NAME: &str = "mike_session_roster";

pub(crate) const MCP_TOOL: &str = "get_workspace_roster";

pub(crate) const DESCRIPTION: &str = "\
List the other Claude Code sessions currently registered against this project, with each \
session's id, working directory, declared goal and liveness. Answered by the mike proxy \
from its own capture database, so it works without any MCP configuration. Use it before \
starting work that another session may already be doing, or when you need to know who \
else is in the tree.";

pub(crate) fn input_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "required": [],
    })
}

pub(crate) async fn answer(state: &AppState, project_path: &str) -> String {
    let Some(persist) = state.persist.as_ref() else {
        return "The mike proxy has persistence disabled, so it has no roster to read.".to_string();
    };

    let project_id = match persist.find_project_id_by_path(project_path).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return format!(
                "No project is registered at {project_path}, so no sessions are on its roster."
            );
        }
        Err(e) => {
            warn_project_lookup_failed(project_path, &e);
            return "The roster could not be read: the project lookup against the capture \
                    database failed."
                .to_string();
        }
    };

    let rows = match persist
        .fetch_workspace_roster(project_id, RosterScope::Live)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            warn_roster_fetch_failed(project_path, &e);
            return "The roster could not be read: the query against the capture database \
                    failed."
                .to_string();
        }
    };

    render(project_path, &rows)
}

fn warn_project_lookup_failed(project_path: &str, e: &impl std::fmt::Display) {
    tracing::warn!(error = %e, project_path, "mike_session_roster: project lookup failed");
}

fn warn_roster_fetch_failed(project_path: &str, e: &impl std::fmt::Display) {
    tracing::warn!(error = %e, project_path, "mike_session_roster: roster fetch failed");
}

fn render(
    project_path: &str,
    rows: &[crate::store::persistence::workspace::WorkspaceRosterRow],
) -> String {
    if rows.is_empty() {
        return format!(
            "No other sessions are registered as live on {project_path}. Either you are the \
             only one working here, or the others never registered — registration is what \
             puts a session on this list, so an unregistered session is invisible to it."
        );
    }

    let mut out = format!(
        "{} session{} registered as live on {project_path}:\n",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    );
    for row in rows {
        out.push_str(&format!(
            "\n- {} ({})",
            row.session_uuid,
            row.liveness.as_str()
        ));
        out.push_str(&format!("\n  working in: {}", row.cwd));
        if let Some(worktree) = row.worktree.as_deref() {
            out.push_str(&format!("\n  worktree: {worktree}"));
        }
        match row.goal.as_deref() {
            Some(goal) if !goal.trim().is_empty() => {
                out.push_str(&format!("\n  goal: {goal}"));
            }
            _ => out.push_str("\n  goal: not declared"),
        }
        if let Some(next) = row.next_steps.as_deref().filter(|s| !s.trim().is_empty()) {
            out.push_str(&format!("\n  next: {next}"));
        }
        if let Some(at) = row.last_request_at {
            out.push_str(&format!("\n  last captured request: {at}"));
        }
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures; hand-built JSON and fixture rows are expected to parse"
)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::store::persistence::workspace::WorkspaceRosterRow;
    use crate::workspace::Liveness;

    fn row(goal: Option<&str>) -> WorkspaceRosterRow {
        WorkspaceRosterRow {
            session_uuid: uuid::Uuid::nil(),
            cwd: "/repo".to_string(),
            worktree: None,
            registered_via: "hook".to_string(),
            registered_at: Utc.timestamp_opt(0, 0).single().expect("epoch"),
            lease_expires_at: Utc.timestamp_opt(0, 0).single().expect("epoch"),
            released_at: None,
            goal: goal.map(str::to_owned),
            next_steps: None,
            display_name: None,
            role: None,
            instructions: None,
            liveness: Liveness::Live,
            last_request_at: None,
            newest_request_number: None,
            channel_present: false,
            channel_cse_id: None,
            heartbeat_age_seconds: None,
            heartbeat_interval_seconds: None,
            idle_seconds: None,
            session_state: None,
            session_state_at: None,
            reply_channel_open: false,
            reply_channel_opened_at: None,
            last_inbound_message_at: None,
            post_turn_summary: None,
            post_turn_summary_at: None,
            client_version: None,
            liveness_source: "lease",
        }
    }

    #[test]
    fn an_empty_roster_says_registration_is_what_puts_a_session_on_it() {
        let text = render("/repo", &[]);
        assert!(text.contains("never registered"), "{text}");
    }

    #[test]
    fn a_row_without_a_goal_says_so_rather_than_omitting_the_line() {
        let text = render("/repo", &[row(None)]);
        assert!(text.contains("goal: not declared"), "{text}");
    }

    #[test]
    fn a_row_renders_its_session_id_liveness_and_goal() {
        let text = render("/repo", &[row(Some("land #204"))]);
        assert!(text.contains(&uuid::Uuid::nil().to_string()), "{text}");
        assert!(text.contains("(live)"), "{text}");
        assert!(text.contains("goal: land #204"), "{text}");
    }

    #[test]
    fn the_count_line_agrees_with_the_row_count() {
        let text = render("/repo", &[row(None), row(None)]);
        assert!(text.starts_with("2 sessions registered as live"), "{text}");
        let one = render("/repo", &[row(None)]);
        assert!(one.starts_with("1 session registered as live"), "{one}");
    }
}
