use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::body::{compose_body, goal_line};
use super::decide::{ReturnVia, choose_return};
use crate::base_state::BaseState;
use crate::launch::backend_choice::CallerTerminal;
use crate::launch::brief_delivery::brief_reminder;
use crate::launch::{ServerOpenRequest, open_for_server};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::conditions::{ConditionRow, Fired, SessionPresence};
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::workspace::QueuedLaunchBrief;

pub struct Outcome {
    pub via: &'static str,
    pub launch_id: Option<i64>,
    pub session: Option<Uuid>,
    pub missed: bool,
}

impl Outcome {
    pub fn to_json(&self) -> Value {
        json!({
            "via": self.via,
            "launch_id": self.launch_id,
            "session": self.session,
            "missed": self.missed,
        })
    }
}

fn target_of(row: &ConditionRow) -> Result<Uuid, String> {
    row.target_session
        .or(row.created_by_session)
        .ok_or_else(|| "the condition has no target session and no registering session".to_owned())
}

async fn thread_context(persist: &PersistHandle, row: &ConditionRow) -> Option<String> {
    let (project_id, slug) = row.project_id.zip(row.thread_slug.as_deref())?;
    match persist
        .fleet()
        .condition_thread_context(project_id, slug)
        .await
    {
        Ok(context) => context,
        Err(error) => {
            tracing::debug!(%error, thread = slug, "conditions: thread context unavailable");
            None
        }
    }
}

pub(super) async fn send_mail(
    state: &BaseState,
    persist: &PersistHandle,
    to: Uuid,
    body: &str,
) -> Result<&'static str, String> {
    let inserted = persist
        .launch()
        .insert_message(Uuid::new_v4(), to, None, body)
        .await
        .map_err(|e| format!("mailing session {to} failed: {e}"))?
        .ok_or_else(|| format!("session {to} is not known to mike, so it cannot be mailed"))?;
    let queued = QueuedLaunchBrief {
        message_uuid: inserted.message_uuid,
        body: body.to_owned(),
        queued_at: inserted.queued_at,
    };
    let pushed = state
        .mail_push()
        .push_or_queue(to, brief_reminder(to, &queued))
        .await;
    Ok(if pushed { "push" } else { "mail" })
}

struct Launch {
    folder: String,
    claude_args: Vec<String>,
    name: Option<String>,
    goal: Option<String>,
    body: String,
}

fn open_request(launch: Launch) -> ServerOpenRequest {
    ServerOpenRequest {
        folder: launch.folder,
        runtime: crate::runtime::RuntimeKind::Claude,
        claude_args: launch.claude_args,
        prompt: Some(launch.body),
        run: None,
        seat: None,
        session_name: launch.name,
        role: None,
        instructions: None,
        goal: launch.goal,
        template_id: None,
        worktree_ok: false,
        trust: false,
        proxy_env: true,
        remote_control: true,
        channels: true,
        backend: None,
        caller: CallerTerminal::default(),
    }
}

async fn open(state: &BaseState, launch: Launch, via: &'static str) -> Result<Outcome, String> {
    let start = open_for_server(state.clone(), open_request(launch))
        .await
        .map_err(|e| format!("starting the session failed: {e}"))?;
    Ok(Outcome {
        via,
        launch_id: start.launch_id,
        session: start.session,
        missed: false,
    })
}

fn start_launch(row: &ConditionRow, folder: String, body: String) -> Launch {
    let slug = row.thread_slug.as_deref().unwrap_or("condition");
    Launch {
        folder,
        claude_args: Vec::new(),
        name: Some(format!("{slug}-wake")),
        goal: Some(goal_line(&row.brief)).filter(|g| !g.is_empty()),
        body,
    }
}

async fn presence_of(persist: &PersistHandle, session: Uuid) -> Option<SessionPresence> {
    persist
        .fleet()
        .condition_session_presence(session)
        .await
        .inspect_err(|error| tracing::debug!(%error, "conditions: presence lookup failed"))
        .ok()
        .flatten()
}

async fn start_session(
    state: &BaseState,
    persist: &PersistHandle,
    row: &ConditionRow,
    body: String,
) -> Result<Outcome, String> {
    let folder = match row.folder.clone().filter(|f| !f.trim().is_empty()) {
        Some(folder) => folder,
        None => {
            let target = target_of(row)?;
            presence_of(persist, target)
                .await
                .and_then(|p| p.cwd)
                .ok_or_else(|| {
                    format!("no folder to start in: the condition has none and session {target} has no known cwd")
                })?
        }
    };
    open(state, start_launch(row, folder, body), "start").await
}

async fn return_to_session(
    state: &BaseState,
    persist: &PersistHandle,
    row: &ConditionRow,
    body: String,
) -> Result<Outcome, String> {
    let target = target_of(row)?;
    let presence = presence_of(persist, target).await;
    match choose_return(presence.as_ref(), row.thread_slug.is_some(), Utc::now()) {
        ReturnVia::Mail => {
            let via = send_mail(state, persist, target, &body).await?;
            Ok(Outcome {
                via,
                launch_id: None,
                session: Some(target),
                missed: false,
            })
        }
        ReturnVia::Resume => {
            let folder = presence.and_then(|p| p.cwd).unwrap_or_default();
            let launch = Launch {
                folder,
                claude_args: vec!["--resume".to_owned(), target.to_string()],
                name: None,
                goal: None,
                body,
            };
            open(state, launch, "resume").await
        }
        ReturnVia::Start => start_session_for_return(state, persist, row, body, presence).await,
    }
}

async fn start_session_for_return(
    state: &BaseState,
    persist: &PersistHandle,
    row: &ConditionRow,
    body: String,
    presence: Option<SessionPresence>,
) -> Result<Outcome, String> {
    let known_cwd = presence.and_then(|p| p.cwd);
    let mut with_folder = row.clone();
    if with_folder
        .folder
        .as_deref()
        .is_none_or(|f| f.trim().is_empty())
    {
        with_folder.folder = known_cwd;
    }
    start_session(state, persist, &with_folder, body).await
}

pub async fn run_action(
    state: &BaseState,
    persist: &PersistHandle,
    row: &ConditionRow,
    fired: Fired,
) -> Result<Outcome, String> {
    let context = thread_context(persist, row).await;
    let body = compose_body(row, fired, context.as_deref());
    let mut outcome = match row.action.as_str() {
        "message" => {
            let target = target_of(row)?;
            let via = send_mail(state, persist, target, &body).await?;
            Outcome {
                via,
                launch_id: None,
                session: Some(target),
                missed: false,
            }
        }
        "return" => return_to_session(state, persist, row, body).await?,
        "start" => start_session(state, persist, row, body).await?,
        other => {
            return Err(format!(
                "action `{other}` is not one of return, start, message"
            ));
        }
    };
    outcome.missed = fired == Fired::Missed;
    Ok(outcome)
}
