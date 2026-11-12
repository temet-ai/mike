use std::sync::LazyLock;

use dashmap::DashMap;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::observation::ManagedObservation;
use super::options::{operator_codex_home, prepare_home};
use super::register::{CODEX_BACKEND, CODEX_BACKEND_REASON, bind_seat_thread, bound_thread};
use super::{CodexError, CodexSeat, SeatOptions};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::{LaunchOutcome, LaunchSurfaceRefs, NewLaunch};

enum Control {
    Brief(String),
    Interrupt(oneshot::Sender<Result<(), String>>),
    Stop(oneshot::Sender<Result<(), String>>),
    Call(String, Value, oneshot::Sender<Result<Value, CodexError>>),
}

static SEATS: LazyLock<DashMap<Uuid, mpsc::Sender<Control>>> = LazyLock::new(DashMap::new);

pub struct SeatLaunch<'a> {
    pub run_id: &'a str,
    pub seat: &'a str,
    pub session: Uuid,
    pub prompt: String,
    pub identity: Value,
}

pub async fn begin(
    persist: &PersistHandle,
    options: SeatOptions,
    launch: SeatLaunch<'_>,
) -> Result<(), String> {
    let launch_id = record_attempt(persist, &options, &launch).await;
    let model = options.model.clone();
    let started = start(persist, options, &launch).await;
    let outcome = match &started {
        Ok(seat) => opened(persist, launch_id, seat).await,
        Err(error) => failed(error.to_string()),
    };
    if let Some(id) = launch_id
        && let Err(error) = persist.record_launch_outcome(id, &outcome).await
    {
        tracing::warn!(error = %error, launch_id = id, "codex seat launch outcome not recorded");
    }
    let seat = started.map_err(|error| error.to_string())?;
    let observer =
        ManagedObservation::begin(persist.clone(), launch.session, seat.thread_id(), model).await;
    let (tx, rx) = mpsc::channel(32);
    SEATS.insert(launch.session, tx);
    tokio::spawn(supervise(launch.session, seat, rx, observer));
    Ok(())
}

async fn start(
    persist: &PersistHandle,
    options: SeatOptions,
    launch: &SeatLaunch<'_>,
) -> anyhow::Result<CodexSeat> {
    prepare_home(&options.codex_home, operator_codex_home().as_deref())?;
    let model = options.model.clone().filter(|m| !m.trim().is_empty());
    let home = options.codex_home.clone();
    let earlier = bound_thread(persist, &home, launch.session).await?;
    let seat = match &earlier {
        Some(thread) => CodexSeat::resume(options, thread).await?,
        None => CodexSeat::launch(options).await?,
    };
    if let Some(model) = model
        && !seat.model_is_listed(&model).await?
    {
        let _ = seat.stop().await;
        anyhow::bail!(
            "model `{model}` is not in this Codex login's model/list; choose a listed model or leave model empty"
        );
    }
    bind_seat_thread(
        persist,
        &home,
        launch.session,
        seat.thread_id(),
        seat.forked_from(),
    )
    .await?;
    let text = match earlier {
        Some(_) => restart_note(&launch.prompt),
        None => launch.prompt.clone(),
    };
    seat.brief(&text).await?;
    Ok(seat)
}

fn restart_note(prompt: &str) -> String {
    format!(
        "mike restarted this seat and resumed your thread. Continue your task from where you were; your brief was:\n\n{prompt}"
    )
}

async fn record_attempt(
    persist: &PersistHandle,
    options: &SeatOptions,
    launch: &SeatLaunch<'_>,
) -> Option<i64> {
    let mut new = NewLaunch::new(CODEX_BACKEND, options.cwd.to_string_lossy());
    new.session_uuid = Some(launch.session);
    new.run_id = Some(launch.run_id.to_string());
    new.seat = Some(launch.seat.to_string());
    new.model = options.model.clone();
    new.claude_args = vec!["codex".into(), "app-server".into()];
    new.prompt_present = !launch.prompt.is_empty();
    new.brief_mode = Some("inline".into());
    new.brief_text = Some(launch.prompt.clone());
    new.remote_control = false;
    new.proxy_env = false;
    new.identity = Some(launch.identity.clone());
    new.backend_reason = Some(CODEX_BACKEND_REASON.into());
    match persist.insert_launch(&new).await {
        Ok(id) => Some(id),
        Err(error) => {
            tracing::warn!(error = %error, session_id = %launch.session, "codex seat launch not recorded");
            None
        }
    }
}

async fn opened(
    persist: &PersistHandle,
    launch_id: Option<i64>,
    seat: &CodexSeat,
) -> LaunchOutcome {
    if let Some(id) = launch_id {
        let surface = LaunchSurfaceRefs {
            workspace_ref: None,
            tab_ref: None,
            surface_ref: Some(format!("codex-thread:{}", seat.thread_id())),
            pid: seat.pid().and_then(|p| i32::try_from(p).ok()),
            actual_placement: Some("headless".into()),
        };
        if let Err(error) = persist.record_launch_opened(id, &surface).await {
            tracing::warn!(error = %error, launch_id = id, "codex seat open not recorded");
        }
    }
    LaunchOutcome {
        result: "success".into(),
        reason: None,
        opened: true,
        delivered: Some(true),
        confirmed: true,
        channel_check: None,
        stderr: None,
    }
}

fn failed(reason: String) -> LaunchOutcome {
    LaunchOutcome {
        result: "failed".into(),
        reason: Some(reason),
        opened: false,
        delivered: None,
        confirmed: false,
        channel_check: None,
        stderr: None,
    }
}

async fn supervise(
    session: Uuid,
    mut seat: CodexSeat,
    mut control: mpsc::Receiver<Control>,
    mut observer: ManagedObservation,
) {
    let mut active_turn: Option<String> = None;
    loop {
        tokio::select! {
            event = seat.next_event() => match event {
                Some(event) => {
                    track_turn(&mut active_turn, &event);
                    observer.event(&event).await;
                },
                None => break,
            },
            order = control.recv() => match order {
                Some(Control::Stop(reply)) => {
                    let _ = reply.send(seat.stop().await.map_err(|e| e.to_string()));
                    SEATS.remove(&session);
                    observer.end().await;
                    return;
                }
                Some(order) => obey(session, &seat, active_turn.as_deref(), order).await,
                None => break,
            },
        }
    }
    SEATS.remove(&session);
    let _ = seat.stop().await;
    observer.end().await;
}

async fn obey(session: Uuid, seat: &CodexSeat, active_turn: Option<&str>, order: Control) {
    match order {
        Control::Brief(text) => {
            if let Err(error) = seat.brief(&text).await {
                tracing::warn!(error = %error, session_id = %session, "codex seat brief failed");
            }
        }
        Control::Interrupt(reply) => {
            let result = match active_turn {
                Some(turn) => seat.interrupt(turn).await.map_err(|e| e.to_string()),
                None => Err(format!("seat {session} has no turn in progress")),
            };
            let _ = reply.send(result);
        }
        Control::Stop(reply) => {
            let _ = reply.send(Err(format!("seat {session} stop arrived outside the loop")));
        }
        Control::Call(method, params, reply) => {
            let _ = reply.send(seat.call(&method, params).await);
        }
    }
}

pub async fn call_seat(session: Uuid, method: &str, params: Value) -> Result<Value, CodexError> {
    let (tx, rx) = oneshot::channel();
    send(session, Control::Call(method.to_string(), params, tx))
        .map_err(|_| CodexError::Closed("seat control"))?;
    rx.await.map_err(|_| CodexError::Closed("seat reply"))?
}

fn track_turn(active: &mut Option<String>, event: &Value) {
    let turn = event["params"]["turn"]["id"].as_str().map(str::to_string);
    match event["method"].as_str() {
        Some("turn/started") => *active = turn,
        Some("turn/completed") if *active == turn => *active = None,
        _ => {}
    }
}

pub fn live_seat(session: Uuid) -> bool {
    SEATS.contains_key(&session)
}

pub fn brief_seat(session: Uuid, text: String) -> Result<(), String> {
    send(session, Control::Brief(text))
}

pub async fn interrupt_seat(session: Uuid) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    send(session, Control::Interrupt(tx))?;
    rx.await.map_err(|_| closed(session))?
}

pub async fn stop_seat(session: Uuid) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    send(session, Control::Stop(tx))?;
    rx.await.map_err(|_| closed(session))?
}

fn send(session: Uuid, order: Control) -> Result<(), String> {
    let sender = SEATS
        .get(&session)
        .ok_or_else(|| format!("no Codex seat runs for session {session} in this mike"))?;
    sender.try_send(order).map_err(|error| match error {
        mpsc::error::TrySendError::Full(_) => {
            format!("the Codex seat control queue for {session} is full")
        }
        mpsc::error::TrySendError::Closed(_) => closed(session),
    })
}

fn closed(session: Uuid) -> String {
    format!("the Codex seat of session {session} has stopped")
}

pub fn identity(seat: &str, role: &str, instructions: &str, goal: &str) -> Value {
    json!({"name": seat, "role": role, "instructions": instructions, "goal": goal})
}
