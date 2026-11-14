use crate::store::persistence::web_repo::WebPersist as _;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::process::Child;

use super::admission;
use super::context::EngineerContext;
use super::process;
use crate::store::persistence::wright::Usage;

const KILL_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunEnd {
    Exited { code: Option<i32> },
    Killed { reason: String },
}

pub(crate) fn breach(usage: &Usage, now: DateTime<Utc>) -> Option<String> {
    if usage.state != "running" {
        return Some(format!(
            "the run's row is `{}`, no longer running",
            usage.state
        ));
    }
    if now >= usage.deadline_at {
        return Some(format!("wall time ran out at {}", usage.deadline_at));
    }
    if usage.tool_calls > usage.max_tool_calls {
        return Some(format!(
            "{} tool calls passed the limit of {}",
            usage.tool_calls, usage.max_tool_calls
        ));
    }
    if usage.tokens > usage.max_tokens {
        return Some(format!(
            "{} tokens passed the limit of {}",
            usage.tokens, usage.max_tokens
        ));
    }
    None
}

struct Watch<'a> {
    ctx: &'a EngineerContext,
    id: i64,
    root: i32,
    deadline: DateTime<Utc>,
    paused: bool,
}

impl Watch<'_> {
    fn next_wake(&self) -> Duration {
        let left = (self.deadline - Utc::now()).to_std().unwrap_or_default();
        left.min(self.ctx.poll)
    }

    async fn check(&mut self) -> Option<String> {
        if !self.ctx.toggles.load().wright {
            return Some("the wright toggle was turned off".to_owned());
        }
        match self.ctx.persist.web().record_wright_usage(self.id).await {
            Ok(Some(usage)) => {
                self.deadline = usage.deadline_at;
                if let Some(reason) = breach(&usage, Utc::now()) {
                    return Some(reason);
                }
            }
            Ok(None) => return Some("the run's row is gone".to_owned()),
            Err(e) => {
                tracing::warn!(error = %e, investigation = self.id, "wright: usage could not be counted this poll")
            }
        }
        if Utc::now() >= self.deadline {
            return Some(format!("wall time ran out at {}", self.deadline));
        }
        self.pace();
        None
    }

    fn pace(&mut self) {
        let busy = admission::machine_busy(&self.ctx.paths.locks());
        match (&busy, self.paused) {
            (Some(reason), false) => self.pause(reason),
            (None, true) => self.resume(),
            _ => {}
        }
    }

    fn pause(&mut self, reason: &str) {
        log_pausing(self.id, reason);
        process::pause_tree(self.root);
        self.paused = true;
    }

    fn resume(&mut self) {
        log_resuming(self.id);
        process::resume_tree(self.root);
        self.paused = false;
    }
}

fn log_pausing(investigation: i64, reason: &str) {
    tracing::info!(investigation, %reason, "wright: pausing the engineer run");
}

fn log_resuming(investigation: i64) {
    tracing::info!(investigation, "wright: resuming the engineer run");
}

async fn stop(child: &mut Child, root: i32, reason: String) -> RunEnd {
    process::kill_tree(root);
    if tokio::time::timeout(KILL_WAIT, child.wait()).await.is_err() {
        tracing::warn!(pid = root, "wright: the sandbox did not exit after SIGKILL");
    }
    RunEnd::Killed { reason }
}

pub(crate) async fn supervise(
    ctx: &EngineerContext,
    id: i64,
    child: &mut Child,
    root: i32,
    deadline: DateTime<Utc>,
) -> RunEnd {
    let mut watch = Watch {
        ctx,
        id,
        root,
        deadline,
        paused: false,
    };
    loop {
        let wake = watch.next_wake();
        tokio::select! {
            status = child.wait() => {
                return RunEnd::Exited { code: status.ok().and_then(|s| s.code()) };
            }
            () = ctx.shutdown.cancelled() => {
                return stop(child, root, "mike is shutting down".to_owned()).await;
            }
            () = tokio::time::sleep(wake) => {}
        }
        if let Some(reason) = watch.check().await {
            return stop(child, root, reason).await;
        }
    }
}
