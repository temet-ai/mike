use crate::store::persistence::web_repo::WebPersist as _;
use chrono::{DateTime, Duration, Utc};

use super::context::Machine;
use super::process;
use crate::store::persistence::wright::LiveRun;
use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) const GRACE: Duration = Duration::minutes(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Keep,
    Reconcile(String),
    KillThenReconcile(i32, String),
}

#[derive(Clone, Copy)]
pub(crate) struct Liveness {
    pub(crate) starter: fn(i32) -> bool,
    pub(crate) sandbox: fn(i32) -> bool,
}

fn same_machine(row: &LiveRun, me: &Machine) -> bool {
    match (row.machine_uuid, me.uuid) {
        (Some(theirs), Some(mine)) => theirs == mine,
        _ => row.host == me.host,
    }
}

fn elsewhere(row: &LiveRun, overdue: bool) -> Verdict {
    if !overdue {
        return Verdict::Keep;
    }
    Verdict::Reconcile(format!(
        "the run on {} passed its deadline {} and the {}-minute grace without ending",
        row.host,
        row.deadline_at,
        GRACE.num_minutes()
    ))
}

fn starting(pid: i32, overdue: bool, alive: bool) -> Verdict {
    if !alive {
        return Verdict::Reconcile(format!(
            "launch uncertain: mike process {pid} wrote the row and is gone before it recorded a sandbox; a sandbox cannot outlive it (--die-with-parent)"
        ));
    }
    if overdue {
        return Verdict::Reconcile(format!(
            "launch uncertain: mike process {pid} has not recorded a sandbox by the deadline and grace"
        ));
    }
    Verdict::Keep
}

fn running(row: &LiveRun, pid: i32, overdue: bool, alive: bool) -> Verdict {
    match (alive, overdue) {
        (false, _) => Verdict::Reconcile(format!(
            "the run's sandbox {pid} is gone (mike restarted or the run died) without recording an end"
        )),
        (true, true) => Verdict::KillThenReconcile(
            pid,
            format!(
                "the run's sandbox {pid} outlived its deadline {} and grace",
                row.deadline_at
            ),
        ),
        (true, false) => Verdict::Keep,
    }
}

pub(crate) fn judge(row: &LiveRun, me: &Machine, now: DateTime<Utc>, live: &Liveness) -> Verdict {
    let overdue = now > row.deadline_at + GRACE;
    if !same_machine(row, me) {
        return elsewhere(row, overdue);
    }
    match (row.state.as_str(), row.pid) {
        (_, None) => Verdict::Reconcile(
            "launch uncertain: the row records no process, so whether a sandbox started is unknown"
                .to_owned(),
        ),
        ("starting", Some(pid)) => starting(pid, overdue, (live.starter)(pid)),
        (_, Some(pid)) => running(row, pid, overdue, (live.sandbox)(pid)),
    }
}

pub(crate) fn mike_alive(pid: i32) -> bool {
    if u32::try_from(pid).is_ok_and(|pid| pid == std::process::id()) {
        return true;
    }
    let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    let program = cmdline.split(|b| *b == 0).next().unwrap_or_default();
    std::path::Path::new(&*String::from_utf8_lossy(program))
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("mike"))
}

pub(crate) async fn reconcile(
    persist: &PersistHandle,
    me: &Machine,
) -> Result<Vec<i64>, PersistError> {
    let live = persist.web().live_wright_runs().await?;
    let now = Utc::now();
    let checks = Liveness {
        starter: mike_alive,
        sandbox: process::bwrap_alive,
    };
    let mut ids = Vec::new();
    let mut reasons = Vec::new();
    for row in &live {
        match judge(row, me, now, &checks) {
            Verdict::Keep => continue,
            Verdict::Reconcile(reason) => reasons.push(reason),
            Verdict::KillThenReconcile(pid, reason) => {
                process::kill_tree(pid);
                reasons.push(reason);
            }
        }
        ids.push(row.id);
    }
    if ids.is_empty() {
        return Ok(ids);
    }
    let done = persist.web().reconcile_wright_runs(&ids, &reasons).await?;
    for id in &done {
        tracing::warn!(
            investigation = id,
            "wright: reconciled an engineer run that did not record its end"
        );
    }
    Ok(done)
}
