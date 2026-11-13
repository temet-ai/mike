use bytes::Bytes;

use crate::api::workspace::{Briefing, BriefingBuildRequest, build_workspace_briefing};
use crate::pipeline::{RequestStage, StageBody, StageContext, is_session_own_lane};
use crate::proxy::engine::workspace_register::await_workspace_registration;
use crate::proxy::remote_control::inject::PushDeps;
use crate::state::AppState;
use crate::store::models::{ReminderSource, RequestId};
use crate::wire::manifest::AppliedStage;

use super::stage::{StageManifestCodes, record_outcome_without_body_change};
use super::{PushOrAppendRequest, RemindersOutcome, push_or_append_reminder};

const BRIEFING_SOURCE: &str = "proxy-workspace-briefing";

const WORKSPACE_BRIEFING_CODES: StageManifestCodes = StageManifestCodes {
    stage: "workspace_briefing",
    policy_id: "workspace_briefing",
    delivered_out_of_band: "workspace_briefing_delivered_out_of_band",
    nothing_to_inject: "workspace_briefing_nothing_to_inject",
    source_finding_ids_are_lesson_ids: false,
};

pub struct WorkspaceBriefingRequest<'a> {
    pub state: &'a AppState,
    pub body_bytes: &'a [u8],
    pub session_id: &'a str,
    pub project_path: &'a str,
    pub request_id: RequestId,
    pub push_deps: &'a PushDeps,
}

pub enum WorkspaceBriefingOutcome {
    AlreadyDelivered,
    NoPersist,
    NoProject,
    RegistrationFailed,
    RosterFailed,
    Delivered {
        outcome: RemindersOutcome,
        peer_count: usize,
    },
}

impl WorkspaceBriefingOutcome {
    fn early_reason_code(&self) -> Option<&'static str> {
        match self {
            Self::AlreadyDelivered => Some("workspace_briefing_already_delivered"),
            Self::NoPersist => Some("workspace_briefing_no_persist"),
            Self::NoProject => Some("workspace_briefing_no_project"),
            Self::RegistrationFailed => Some("workspace_briefing_registration_failed"),
            Self::RosterFailed => Some("workspace_briefing_roster_failed"),
            Self::Delivered { .. } => None,
        }
    }
}

pub async fn apply_workspace_briefing(
    req: WorkspaceBriefingRequest<'_>,
) -> WorkspaceBriefingOutcome {
    let WorkspaceBriefingRequest {
        state,
        body_bytes,
        session_id,
        project_path,
        request_id,
        push_deps,
    } = req;

    if state.workspace_briefing_delivered.contains(session_id)
        || super::client_context::client_text_reached(body_bytes, BRIEFING_SOURCE)
    {
        return WorkspaceBriefingOutcome::AlreadyDelivered;
    }
    let (briefing, peer_count) =
        match render_workspace_briefing(state, session_id, project_path).await {
            Ok(rendered) => rendered,
            Err(early) => return early,
        };
    let reminder_text = briefing_reminder(&briefing.text());
    let outcome = push_or_append_reminder(PushOrAppendRequest {
        push_deps,
        session_id,
        body_bytes,
        source: ReminderSource::WorkspaceBriefing,
        reminder_text,
        source_finding_ids: Vec::new(),
        source_request_id: 0,
        request_id,
    })
    .await;
    WorkspaceBriefingOutcome::Delivered {
        outcome,
        peer_count,
    }
}

pub async fn render_workspace_briefing(
    state: &AppState,
    session_id: &str,
    project_path: &str,
) -> Result<(Briefing, usize), WorkspaceBriefingOutcome> {
    let Some(handle) = state.persist() else {
        return Err(WorkspaceBriefingOutcome::NoPersist);
    };
    if project_path.is_empty() {
        return Err(WorkspaceBriefingOutcome::NoProject);
    }
    let Some(registered) = await_workspace_registration(state, session_id, project_path).await
    else {
        return Err(WorkspaceBriefingOutcome::RegistrationFailed);
    };
    let Ok(self_session_uuid) = session_id.parse::<uuid::Uuid>() else {
        return Err(WorkspaceBriefingOutcome::RegistrationFailed);
    };

    let busy_ids = state.in_flight.busy_session_ids();
    let own_path = match registered.project_path.as_deref() {
        Some(stored) => Some(
            path_on_machine(
                handle,
                registered.project_id,
                stored,
                state.resolve_host_machine_uuid().await,
            )
            .await,
        ),
        None => None,
    };
    let build_result = build_workspace_briefing(BriefingBuildRequest {
        handle,
        busy_ids: &busy_ids,
        self_session_uuid,
        project_id: registered.project_id,
        project_path: own_path.as_deref(),
        registered_via: registered.registered_via,
        intent: None,
        cwd: project_path,
    })
    .await;
    let Ok((briefing, roster)) = build_result else {
        return Err(WorkspaceBriefingOutcome::RosterFailed);
    };

    let peer_count = roster.iter().filter(|r| !r.is_self).count();
    Ok((briefing, peer_count))
}

pub const NO_LOCATION_ON_THIS_MACHINE: &str =
    "this machine has no registered location for this project";

pub async fn path_on_machine(
    handle: &crate::store::persistence::PersistHandle,
    project_id: i64,
    stored: &str,
    machine: Option<uuid::Uuid>,
) -> String {
    let Some(machine) = machine else {
        return stored.to_owned();
    };
    match handle.machine_location_root(project_id, machine).await {
        Ok(Some(root)) => root,
        Ok(None) => NO_LOCATION_ON_THIS_MACHINE.to_owned(),
        Err(error) => {
            tracing::warn!(%error, project_id, "workspace briefing: could not read this machine's project location; showing the stored path");
            stored.to_owned()
        }
    }
}

pub fn briefing_reminder(text: &str) -> String {
    format!(
        "<system-reminder source=\"{BRIEFING_SOURCE}\">\n{}\n</system-reminder>",
        text.trim_end()
    )
}

pub struct WorkspaceBriefingStage;

#[async_trait::async_trait]
impl RequestStage for WorkspaceBriefingStage {
    fn name(&self) -> &'static str {
        "workspace_briefing"
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.workspace_briefing && is_session_own_lane(ctx) && !ctx.is_tool_loop
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let role = super::seat_role::seat_role(ctx.state, ctx.session_id).await;
        if role == super::seat_role::SeatRole::Template {
            body.record_unchanged(
                "workspace_briefing",
                "workspace_briefing",
                "workspace_briefing_template_session",
            );
            return;
        }
        let push_deps = ctx.state.push_deps();
        let outcome = apply_workspace_briefing(WorkspaceBriefingRequest {
            state: ctx.state,
            body_bytes: body.body(),
            session_id: ctx.session_id,
            project_path: ctx.project_path,
            request_id: ctx.request_id,
            push_deps: &push_deps,
        })
        .await;
        let outcome = match outcome {
            WorkspaceBriefingOutcome::Delivered {
                outcome: appended,
                peer_count,
            } => WorkspaceBriefingOutcome::Delivered {
                outcome: super::fork_prefix::keep_inherited_prefix(role, body.body(), appended),
                peer_count,
            },
            other => other,
        };

        if let Some(reason_code) = outcome.early_reason_code() {
            body.record_unchanged("workspace_briefing", "workspace_briefing", reason_code);
            return;
        }
        let WorkspaceBriefingOutcome::Delivered {
            outcome: rem_outcome,
            peer_count,
        } = outcome
        else {
            return;
        };

        let RemindersOutcome::Appended {
            body: new_body,
            reminder_text,
            ..
        } = rem_outcome
        else {
            let delivered = matches!(rem_outcome, RemindersOutcome::DeliveredOutOfBand { .. });
            record_outcome_without_body_change(body, &WORKSPACE_BRIEFING_CODES, &rem_outcome);
            if delivered {
                ctx.state
                    .workspace_briefing_delivered
                    .insert(ctx.session_id.to_owned());
            }
            return;
        };

        ctx.state
            .workspace_briefing_delivered
            .insert(ctx.session_id.to_owned());
        let new_bytes = Bytes::from(new_body);
        tracing::debug!(
            request_id = %ctx.request_id,
            session_id = %ctx.session_id,
            peer_count,
            reminder_len = reminder_text.len(),
            "workspace briefing injected <system-reminder> block"
        );
        body.rewrite(
            new_bytes.clone(),
            AppliedStage {
                stage: "workspace_briefing",
                policy_id: "workspace_briefing",
                policy_version: 1,
                resolved_parameters: serde_json::json!({
                    "project_path": ctx.project_path,
                    "peer_count": peer_count,
                }),
                new_bytes: new_bytes.as_ref(),
                targets: Vec::new(),
                reason_code: "workspace_briefing_injected",
                detail: Some(format!("injected {} bytes", reminder_text.len())),
            },
        );
    }
}

#[cfg(test)]
#[path = "workspace_briefing_db_tests.rs"]
pub mod db_tests;
