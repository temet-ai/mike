use std::future::Future;
use std::sync::LazyLock;
use std::time::Duration;

use dashmap::DashSet;
use serde::Serialize;
use tokio::time::Instant;
use uuid::Uuid;

use crate::api::client_serves::by_deadline;
use crate::api::workspace::{Briefing, protocol_text};
use crate::launch::session_tail::session_tail_lines;
use crate::state::AppState;
use crate::store::updater::analysis_completed;

use super::change_record_tail::tail_reminder_text;
use super::formatter::build_reminder_text;
use super::latest_findings;
use super::seat_role::{SeatRole, seat_role};
use super::self_identity::build_self_identity_reminder_text;
use super::workspace_briefing::{briefing_reminder, render_workspace_briefing};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ContextBlock {
    pub stage: &'static str,
    pub text: String,
}

const CONTEXT_BUDGET: Duration = Duration::from_millis(200);
const FINDINGS_READY_BUDGET: Duration = Duration::from_millis(50);

static REMINDERS_SERVED: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub fn reminders_served_by_client(session_id: &str) -> bool {
    REMINDERS_SERVED.contains(session_id)
}

pub fn client_text_reached(body: &[u8], text: &str) -> bool {
    let needle = text.as_bytes();
    !needle.is_empty() && body.windows(needle.len()).any(|window| window == needle)
}

struct Gathered {
    role: Option<SeatRole>,
    briefing: Option<Briefing>,
    tail: Option<Option<Vec<String>>>,
    findings: Option<String>,
}

async fn when<T>(wanted: bool, work: impl Future<Output = Option<T>>) -> Option<T> {
    if wanted { work.await } else { None }
}

async fn gather(
    state: &AppState,
    session: Uuid,
    session_id: &str,
    project_path: &str,
    budget: Duration,
) -> Gathered {
    let deadline = Instant::now() + budget;
    let toggles = state.resolve_session_toggles(session_id);
    let serves = |key| crate::state::client_serves(state, key);
    let wants_briefing = toggles.workspace_briefing && serves("client_serves_workspace_briefing");
    let wants_tail = state.feature_toggle_overlay.load().change_record_tail
        && serves("client_serves_change_record_tail");
    let wants_findings = toggles.reminders && serves("client_serves_reminders");
    let (state, id, project) = (
        state.clone(),
        session_id.to_owned(),
        project_path.to_owned(),
    );
    let (role_state, role_id) = (state.clone(), id.clone());
    let (briefing_state, briefing_id) = (state.clone(), id.clone());
    let tail_state = state.clone();
    let (role, briefing, tail, findings) = tokio::join!(
        by_deadline(
            deadline,
            async move { seat_role(&role_state, &role_id).await }
        ),
        when(
            wants_briefing,
            by_deadline(deadline, async move {
                render_workspace_briefing(&briefing_state, &briefing_id, &project).await
            }),
        ),
        when(
            wants_tail,
            by_deadline(deadline, async move {
                let persist = tail_state.persist()?;
                Some(session_tail_lines(persist, session).await)
            }),
        ),
        when(
            wants_findings,
            by_deadline(
                deadline,
                async move { latest_findings(&state.store, &id).await }
            ),
        ),
    );
    Gathered {
        role,
        briefing: briefing.and_then(Result::ok).map(|(text, _)| text),
        tail: tail.flatten(),
        findings: findings
            .flatten()
            .and_then(|(_, findings)| build_reminder_text(&findings)),
    }
}

pub async fn prompt_context(
    state: &AppState,
    session: Uuid,
    session_id: &str,
    project_path: &str,
) -> Vec<ContextBlock> {
    prompt_context_within(state, session, session_id, project_path, CONTEXT_BUDGET).await
}

pub async fn prompt_context_within(
    state: &AppState,
    session: Uuid,
    session_id: &str,
    project_path: &str,
    budget: Duration,
) -> Vec<ContextBlock> {
    let gathered = gather(state, session, session_id, project_path, budget).await;
    if !matches!(gathered.role, Some(SeatRole::Plain | SeatRole::Fork)) {
        return Vec::new();
    }
    let toggles = state.resolve_session_toggles(session_id);
    let mut blocks = Vec::new();
    if toggles.self_identity_reminder
        && crate::state::client_serves(state, "client_serves_self_identity")
    {
        state.self_identity_served.insert(session_id.to_owned());
        blocks.push(ContextBlock {
            stage: "self_identity",
            text: build_self_identity_reminder_text(session_id),
        });
    }
    if let Some(briefing) = gathered.briefing {
        state
            .workspace_briefing_served
            .insert(session_id.to_owned());
        blocks.extend(briefing_blocks(&briefing));
    }
    if let Some(lines) = gathered.tail {
        state
            .change_record_tail_served
            .insert(session_id.to_owned());
        blocks.extend(lines.map(|lines| ContextBlock {
            stage: "change_record_tail",
            text: tail_reminder_text(&lines),
        }));
    }
    if let Some(text) = gathered.findings {
        REMINDERS_SERVED.insert(session_id.to_owned());
        blocks.push(ContextBlock {
            stage: "reminders",
            text,
        });
    }
    blocks
}

pub fn briefing_blocks(briefing: &Briefing) -> Vec<ContextBlock> {
    let mut blocks = Vec::new();
    if briefing.protocol {
        blocks.push(ContextBlock {
            stage: "workspace_protocol",
            text: format!(
                "<system-reminder source=\"proxy-workspace-protocol\">\n{}\n</system-reminder>",
                protocol_text(briefing.fleet_seat)
            ),
        });
    }
    blocks.push(ContextBlock {
        stage: "workspace_identity",
        text: briefing_reminder(&briefing.identity),
    });
    if !briefing.roster.trim().is_empty() {
        blocks.push(ContextBlock {
            stage: "workspace_briefing",
            text: briefing_reminder(&briefing.roster),
        });
    }
    blocks
}

pub async fn findings_ready_within_budget(state: &AppState, session_id: &str) -> bool {
    let (state, id) = (state.clone(), session_id.to_owned());
    let deadline = Instant::now() + FINDINGS_READY_BUDGET;
    by_deadline(
        deadline,
        async move { findings_ready_for(&state, &id).await },
    )
    .await
    .unwrap_or(false)
}

pub async fn findings_ready_for(state: &AppState, session_id: &str) -> bool {
    let guard = state.store.read().await;
    let mut ids = guard.requests_by_session_id(session_id);
    ids.pop().is_none_or(|id| analysis_completed(&id))
}
