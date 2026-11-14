use std::path::PathBuf;
use std::time::Duration;

use tokio::io::AsyncReadExt;

use super::managed_command::{self, Context};
use super::owned_execution::{Owner, Phase};
use super::process_tripwire::Setting;

#[path = "managed_process_capture.rs"]
pub mod capture;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "managed_process_group.rs"]
pub mod group;

#[derive(Clone)]
pub struct Command {
    pub slot: PathBuf,
    pub text: String,
    pub run: String,
    pub budget: (Duration, String),
    pub log: Option<PathBuf>,
    pub setting: Option<Setting>,
}

struct CancelOnDrop(Option<Owner>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(owner) = self.0.take() {
            owner.cancel("command caller ended before cleanup completed");
        }
    }
}

pub async fn run(command: Command) -> (Option<i32>, String) {
    let declared = managed_command::current().is_ok_and(|context| {
        context
            .operation
            .as_ref()
            .is_some_and(|op| op.timeout_seconds.is_some())
    });
    let first = run_once(command.clone()).await;
    if declared && super::supervise::timed_out(&first) {
        tracing::warn!(run = %command.run, "fleet supervisor: a declared check passed its timeout and was killed; it is requeued once");
        return run_once(command).await;
    }
    first
}

async fn run_once(mut command: Command) -> (Option<i32>, String) {
    let context = match managed_command::current() {
        Ok(context) => context,
        Err(why) => return (None, why),
    };
    if let Some(seconds) = context.operation.as_ref().and_then(|op| op.timeout_seconds) {
        command.budget.0 = command.budget.0.min(Duration::from_secs(seconds));
    }
    let mut cancel = CancelOnDrop(Some(context.owner.clone()));
    let tracker = context.tracker.clone();
    let owner = context.owner.clone();
    let result = tracker.spawn(supervise(context, command)).await;
    match result {
        Ok(result) => {
            cancel.0 = None;
            result
        }
        Err(error) => {
            quarantine(
                &owner,
                &format!("managed process owner failed: {error}; cleanup is unverified"),
            )
            .await
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn supervise(_context: Context, _command: Command) -> (Option<i32>, String) {
    (
        None,
        "managed process ownership is unsupported on this platform; command was not launched"
            .into(),
    )
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn supervise(context: Context, command: Command) -> (Option<i32>, String) {
    let declared = context
        .operation
        .as_ref()
        .and_then(|op| op.timeout_seconds)
        .map(Duration::from_secs);
    let admitted = match super::managed_queue::waiting_limit(declared) {
        Some(limit) => tokio::time::timeout(limit, managed_command::admit(&context))
            .await
            .ok(),
        None => Some(managed_command::admit(&context).await),
    };
    let permit = match admitted {
        Some(Ok(permit)) => permit,
        Some(Err(why)) => return (None, why),
        None => return (None, "timed out waiting for operation resources".into()),
    };
    if let Err(why) = context.owner.hold_permit(permit) {
        return quarantine(&context.owner, &why).await;
    }
    let queue_ms = context.queued_at.elapsed().as_millis();
    let execution_started = std::time::Instant::now();
    let mut capture = capture::Capture::new(command.log.as_deref());
    let mut owned = match spawn(&context, &command).await {
        Ok(child) => child,
        Err(why) => {
            context.owner.release_cleaned_permit();
            capture.diagnostic(&why);
            return (None, capture.text());
        }
    };
    let Some(mut output) = owned.child.stdout.take() else {
        let reaped = owned.terminate_and_reap(Some(&context.owner)).await;
        if let Err(why) = settle(&context.owner, &mut owned, reaped).await {
            return quarantine(&context.owner, &why).await;
        }
        context.owner.process_finished();
        context.owner.release_cleaned_permit();
        return (
            None,
            "owned command had no output pipe; process cleanup completed".into(),
        );
    };
    let observed = super::process_tripwire::register(
        &command.run,
        &command.slot,
        &command.text,
        command.setting,
        match owned.id() {
            Ok(id) => id,
            Err(why) => return quarantine(&context.owner, &why).await,
        },
    );
    let diagnostic = collect(
        &context.owner,
        &mut owned,
        &mut output,
        &mut capture,
        &command,
    )
    .await;
    drop(observed);
    let ended = Ended {
        diagnostic,
        queue_ms,
        execution_started,
    };
    reap_and_report(&context, &mut owned, &mut output, capture, ended).await
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct Ended {
    diagnostic: Option<String>,
    queue_ms: u128,
    execution_started: std::time::Instant,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn reap_and_report(
    context: &Context,
    owned: &mut group::ChildGroup,
    output: &mut tokio::process::ChildStdout,
    mut capture: capture::Capture,
    ended: Ended,
) -> (Option<i32>, String) {
    let cleanup = owned.terminate_and_reap(Some(&context.owner));
    let (reaped, output_complete) = tokio::join!(cleanup, capture.drain(output));
    let recovered = reaped.as_ref().err().cloned();
    let code = match settle(&context.owner, owned, reaped).await {
        Ok(code) => code,
        Err(why) => return quarantine(&context.owner, &why).await,
    };
    let diagnostic = ended.diagnostic.or_else(|| {
        recovered.map(|why| format!("{why}; the process group was later verified gone"))
    });
    context.owner.process_finished();
    context.owner.release_cleaned_permit();
    super::operation_timing::completed(
        context,
        ended.queue_ms,
        ended.execution_started.elapsed().as_millis(),
    );
    if let Some(why) = &diagnostic {
        capture.diagnostic(why);
    }
    if diagnostic.is_some() || !output_complete {
        (None, capture.text())
    } else {
        (code, capture.text())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn spawn(context: &Context, command: &Command) -> Result<group::ChildGroup, String> {
    let _local = context.owner.fence().await;
    let durable = context
        .authority
        .lock()
        .await
        .map_err(|why| managed_command::spawn_refused(&context.owner, why))?;
    if context.owner.token().is_cancelled() {
        return Err(managed_command::cancelled(&context.owner));
    }
    let mut process = tokio::process::Command::new("sh");
    process
        .arg("-c")
        .arg(format!("({}) 2>&1", command.text))
        .current_dir(&command.slot)
        .env("MIKE_RUN_ID", &command.run)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .process_group(0);
    for name in &context.strip_environment {
        process.env_remove(name);
    }
    if let Some(operation) = &context.operation {
        process.envs(&operation.environment);
    }
    let child = process
        .spawn()
        .map_err(|error| format!("{} could not start: {error}", command.budget.1))?;
    context
        .owner
        .process_started_with(context.held_resources.clone());
    let child = match group::ChildGroup::new(child) {
        Ok(child) => child,
        Err(why) => {
            context.owner.phase(Phase::CleanupBlocked, Some(why));
            drop(durable);
            drop(_local);
            return std::future::pending().await;
        }
    };
    context.owner.phase(Phase::Executing, None);
    super::managed_queue::executing(context);
    super::check::mark_running(&context.ledger);
    if let Err(why) = durable.commit().await {
        context
            .owner
            .cancel(format!("spawn fence release failed: {why}"));
    }
    Ok(child)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn collect(
    owner: &Owner,
    child: &mut group::ChildGroup,
    output: &mut tokio::process::ChildStdout,
    capture: &mut capture::Capture,
    command: &Command,
) -> Option<String> {
    let deadline = tokio::time::sleep(command.budget.0);
    tokio::pin!(deadline);
    let mut observation = tokio::time::interval(Duration::from_millis(20));
    let mut buffer = [0u8; 16 * 1024];
    let mut eof = false;
    loop {
        tokio::select! {
            biased;
            _ = owner.token().cancelled() => return Some(managed_command::cancelled(owner)),
            _ = &mut deadline => return Some(format!("{} was stopped after {} seconds", command.budget.1, command.budget.0.as_secs())),
            _ = observation.tick() => match child.exited_unreaped() {
                Ok(true) => return None,
                Ok(false) => {},
                Err(why) => return Some(why),
            },
            result = output.read(&mut buffer), if !eof => match result {
                Ok(0) => eof = true,
                Ok(count) => capture.append(&buffer[..count]),
                Err(error) => return Some(format!("managed output read failed: {error}")),
            },
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn settle(
    owner: &Owner,
    owned: &mut group::ChildGroup,
    reaped: Result<Option<i32>, String>,
) -> Result<Option<i32>, String> {
    let why = match reaped {
        Ok(code) => return Ok(code),
        Err(why) => why,
    };
    owner.phase(
        Phase::CleanupBlocked,
        Some(format!(
            "{why}; the admission permit is held until the process group is verified gone"
        )),
    );
    let mut delay = RECHECK_FIRST;
    while owned.id().is_ok() {
        tokio::time::sleep(delay).await;
        if let Ok(code) = owned.terminate_and_reap(None).await {
            return Ok(code);
        }
        delay = (delay * 2).min(RECHECK_LONGEST);
    }
    Err(why)
}

const RECHECK_FIRST: Duration = Duration::from_millis(200);
const RECHECK_LONGEST: Duration = Duration::from_secs(30);

async fn quarantine(owner: &Owner, why: &str) -> (Option<i32>, String) {
    owner.phase(
        Phase::CleanupBlocked,
        Some(format!(
            "{why}; the admission permit remains held and receipt is unresolved"
        )),
    );
    std::future::pending().await
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[path = "managed_process_tests.rs"]
mod tests;
