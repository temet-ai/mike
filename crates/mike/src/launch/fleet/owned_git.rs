use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use super::managed_command::{self, Context};
use super::owned_execution::{Owner, Phase};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::managed_process::group::ChildGroup;

struct Request {
    repo: PathBuf,
    args: Vec<String>,
    context: Option<Context>,
    cancelled: CancellationToken,
    limit: Duration,
    disable_hooks: bool,
}

struct CancelOnDrop(Option<CancellationToken>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(token) = self.0.take() {
            token.cancel();
        }
    }
}

pub(super) async fn run(repo: &Path, args: &[&str]) -> Result<String, String> {
    run_with_hooks(repo, args, true).await
}

pub(super) async fn run_plain(repo: &Path, args: &[&str]) -> Result<String, String> {
    run_with_hooks(repo, args, false).await
}

async fn run_with_hooks(repo: &Path, args: &[&str], disable_hooks: bool) -> Result<String, String> {
    let context = managed_command::current().ok();
    let token = context
        .as_ref()
        .map_or_else(CancellationToken::new, |context| {
            context.owner.token().child_token()
        });
    if token.is_cancelled() {
        return Err("isolated git cancelled before launch".into());
    }
    let request = Request {
        repo: repo.into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        context,
        cancelled: token,
        limit: Duration::from_secs(60),
        disable_hooks,
    };
    run_request(request).await
}

async fn run_request(request: Request) -> Result<String, String> {
    let mut cancel = CancelOnDrop(Some(request.cancelled.clone()));
    let task = match request
        .context
        .as_ref()
        .map(|context| context.tracker.clone())
    {
        Some(tracker) => tracker.spawn(supervise(request)),
        None => tokio::spawn(supervise(request)),
    };
    let result = task
        .await
        .map_err(|error| format!("isolated git owner failed; cleanup is unverified: {error}"));
    cancel.0 = None;
    result?
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn supervise(_request: Request) -> Result<String, String> {
    Err("retained-child Git supervision is unsupported on this platform".into())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn supervise(request: Request) -> Result<String, String> {
    let owner = request.context.as_ref().map(|context| &context.owner);
    if let Some(owner) = owner {
        owner.phase(Phase::Preparing, None);
    }
    let mut child = spawn(&request).await?;
    let (Some(mut stdout), Some(mut stderr)) =
        (child.child.stdout.take(), child.child.stderr.take())
    else {
        if let Err(why) = child.terminate_and_reap(owner).await {
            return quarantine(owner, why).await;
        }
        if let Some(owner) = owner {
            owner.process_finished();
        }
        return Err("isolated git output pipes were missing".into());
    };
    let mut captured = Bytes::default();
    let diagnostic = collect(
        &request,
        &mut child,
        (&mut stdout, &mut stderr),
        &mut captured,
    )
    .await;
    let cleanup = child.terminate_and_reap(owner);
    let drain = captured.drain((&mut stdout, &mut stderr));
    let (status, drained) = tokio::join!(cleanup, drain);
    let status = match status {
        Ok(status) => status,
        Err(why) => return quarantine(owner, why).await,
    };
    if let Some(owner) = owner {
        owner.process_finished();
    }
    if let Some(why) = diagnostic {
        return Err(captured.failure(&why));
    }
    if let Err(why) = drained {
        return Err(captured.failure(&why));
    }
    if status != Some(0) {
        return Err(captured.failure(&format!(
            "isolated git {:?} exited {status:?}",
            request.args
        )));
    }
    Ok(String::from_utf8_lossy(&captured.stdout).trim().to_string())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn spawn(request: &Request) -> Result<ChildGroup, String> {
    let (local, durable) = match &request.context {
        Some(context) => {
            let local = context.owner.fence().await;
            let durable = context
                .authority
                .lock()
                .await
                .map_err(|why| managed_command::spawn_refused(&context.owner, why))?;
            (Some(local), Some(durable))
        }
        None => (None, None),
    };
    if request.cancelled.is_cancelled() {
        return Err("isolated git cancelled before launch".into());
    }
    let mut command = tokio::process::Command::new("git");
    if request.disable_hooks {
        command.args(["-c", "core.hooksPath=/dev/null"]);
    }
    command
        .arg("-C")
        .arg(&request.repo)
        .args(&request.args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    let child = command
        .spawn()
        .map_err(|error| format!("isolated git could not start: {error}"))?;
    let owner = request.context.as_ref().map(|context| &context.owner);
    if let Some(context) = &request.context {
        context
            .owner
            .process_started_with(context.held_resources.clone());
    }
    let child = match ChildGroup::new(child) {
        Ok(child) => child,
        Err(why) => {
            drop(durable);
            drop(local);
            return quarantine(owner, why).await;
        }
    };
    if let Some(fence) = durable
        && let Err(why) = fence.commit().await
    {
        if let Some(owner) = owner {
            owner.cancel(format!("isolated git spawn fence failed: {why}"));
        }
        request.cancelled.cancel();
    }
    Ok(child)
}

#[derive(Default)]
struct Bytes {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_eof: bool,
    stderr_eof: bool,
}

impl Bytes {
    fn failure(&self, why: &str) -> String {
        format!(
            "{why}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&self.stdout),
            String::from_utf8_lossy(&self.stderr)
        )
    }

    async fn drain(
        &mut self,
        (stdout, stderr): (
            &mut tokio::process::ChildStdout,
            &mut tokio::process::ChildStderr,
        ),
    ) -> Result<(), String> {
        let drained = async {
            let (out, err) = tokio::join!(
                stdout.read_to_end(&mut self.stdout),
                stderr.read_to_end(&mut self.stderr)
            );
            out.map_err(|error| error.to_string())?;
            err.map_err(|error| error.to_string())?;
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(1), drained)
            .await
            .map_err(|_| {
                "isolated git output drain timed out; captured output is incomplete".to_string()
            })?
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn collect(
    request: &Request,
    child: &mut ChildGroup,
    (stdout, stderr): (
        &mut tokio::process::ChildStdout,
        &mut tokio::process::ChildStderr,
    ),
    captured: &mut Bytes,
) -> Option<String> {
    let deadline = tokio::time::sleep(request.limit);
    tokio::pin!(deadline);
    let mut observation = tokio::time::interval(Duration::from_millis(20));
    let mut out = [0u8; 16 * 1024];
    let mut err = [0u8; 16 * 1024];
    loop {
        tokio::select! {
            biased;
            _ = request.cancelled.cancelled() => return Some("isolated git cancelled".into()),
            _ = &mut deadline => return Some(format!("isolated git {:?} timed out after {} seconds", request.args, request.limit.as_secs_f64())),
            _ = observation.tick() => match child.exited_unreaped() {
                Ok(true) => return None,
                Ok(false) => {},
                Err(why) => return Some(why),
            },
            result = stdout.read(&mut out), if !captured.stdout_eof => match result {
                Ok(0) => captured.stdout_eof = true,
                Ok(count) => captured.stdout.extend_from_slice(&out[..count]),
                Err(error) => return Some(format!("isolated git stdout read failed: {error}")),
            },
            result = stderr.read(&mut err), if !captured.stderr_eof => match result {
                Ok(0) => captured.stderr_eof = true,
                Ok(count) => captured.stderr.extend_from_slice(&err[..count]),
                Err(error) => return Some(format!("isolated git stderr read failed: {error}")),
            },
        }
    }
}

fn blocked(owner: Option<&Owner>, why: String) -> String {
    if let Some(owner) = owner {
        owner.phase(Phase::CleanupBlocked, Some(why.clone()));
    }
    why
}

async fn quarantine<T>(owner: Option<&Owner>, why: String) -> T {
    let why = blocked(owner, why);
    tracing::error!(%why, "isolated git cleanup is unverified; retained-child owner remains alive");
    std::future::pending().await
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[path = "owned_git_tests.rs"]
mod tests;

#[cfg(feature = "test-seams")]
pub(super) async fn run_fixture(
    repo: &Path,
    args: &[&str],
    limit: Duration,
    cancelled: CancellationToken,
) -> Result<String, String> {
    run_request(Request {
        repo: repo.into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        context: None,
        cancelled,
        limit,
        disable_hooks: true,
    })
    .await
}
