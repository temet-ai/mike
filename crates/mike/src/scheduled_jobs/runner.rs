use std::collections::VecDeque;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

use super::jobs::Job;

const DRAIN_GRACE: Duration = Duration::from_secs(2);
const TAIL_LINES: usize = 20;
const LAUNCH: &str = r#"umask 077 && level="$1" && shift && exec nice -n "$level" "$@""#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunResult {
    Succeeded,
    Alarmed(i32),
    Exited(i32),
    Signalled,
    TimedOut(Duration),
    Unrunnable(String),
}

impl RunResult {
    pub(crate) fn exit_code(&self) -> Option<i32> {
        match self {
            Self::Succeeded => Some(0),
            Self::Alarmed(code) | Self::Exited(code) => Some(*code),
            Self::Signalled | Self::TimedOut(_) | Self::Unrunnable(_) => None,
        }
    }

    pub(crate) fn completed(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Alarmed(_))
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Succeeded => "succeeded".to_owned(),
            Self::Alarmed(code) => format!("finished and raised an alarm (exit code {code})"),
            Self::Exited(code) => format!("exited with code {code}"),
            Self::Signalled => "was ended by a signal".to_owned(),
            Self::TimedOut(limit) => {
                format!("ran past {} minutes and was stopped", limit.as_secs() / 60)
            }
            Self::Unrunnable(why) => format!("could not be run: {why}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RunReport {
    pub(super) result: RunResult,
    pub(super) duration: Duration,
    pub(super) tail: Vec<String>,
}

fn classify(status: ExitStatus) -> RunResult {
    match status.code() {
        Some(0) => RunResult::Succeeded,
        Some(code) => RunResult::Exited(code),
        None => RunResult::Signalled,
    }
}

fn alarm_or_failure(result: RunResult, alarm_exit: Option<i32>) -> RunResult {
    match (result, alarm_exit) {
        (RunResult::Exited(code), Some(alarm)) if code == alarm => RunResult::Alarmed(code),
        (other, _) => other,
    }
}

async fn wait_for(child: &mut Child, limit: Duration) -> RunResult {
    match timeout(limit, child.wait()).await {
        Ok(Ok(status)) => classify(status),
        Ok(Err(e)) => RunResult::Unrunnable(format!("waiting for the script failed: {e}")),
        Err(_) => {
            if let Err(e) = child.kill().await {
                log_kill_failed(&e);
            }
            RunResult::TimedOut(limit)
        }
    }
}

fn log_kill_failed(e: &std::io::Error) {
    tracing::warn!(error = %e, "scheduled_jobs: could not stop the script after its time limit");
}

fn log_read_stopped(e: &std::io::Error) {
    tracing::debug!(error = %e, "scheduled_jobs: reading the script output stopped");
}

async fn collect<R: AsyncRead + Unpin>(reader: Option<R>, ring: &mut VecDeque<String>) {
    let Some(reader) = reader else {
        return;
    };
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line).await {
            Ok(0) => return,
            Ok(_) => {
                if ring.len() == TAIL_LINES {
                    ring.pop_front();
                }
                ring.push_back(String::from_utf8_lossy(&line).trim_end().to_owned());
            }
            Err(e) => {
                log_read_stopped(&e);
                return;
            }
        }
    }
}

fn last_lines(first: VecDeque<String>, second: VecDeque<String>) -> Vec<String> {
    let all: Vec<String> = first.into_iter().chain(second).collect();
    let skip = all.len().saturating_sub(TAIL_LINES);
    all.into_iter().skip(skip).collect()
}

fn unrunnable(why: String, started: Instant) -> RunReport {
    RunReport {
        result: RunResult::Unrunnable(why),
        duration: started.elapsed(),
        tail: Vec::new(),
    }
}

pub(super) async fn run_job(root: &Path, job: &Job) -> RunReport {
    let started = Instant::now();
    let script = root.join(job.script);
    if !script.is_file() {
        return unrunnable(format!("{} does not exist", script.display()), started);
    }
    let spawned = Command::new("sh")
        .arg("-c")
        .arg(LAUNCH)
        .arg("sh")
        .arg(job.nice.to_string())
        .arg(&script)
        .args(job.args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => return unrunnable(format!("starting it failed: {e}"), started),
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut out_ring = VecDeque::new();
    let mut err_ring = VecDeque::new();
    let result = {
        let collecting = async {
            tokio::join!(
                collect(stdout, &mut out_ring),
                collect(stderr, &mut err_ring)
            );
        };
        tokio::pin!(collecting);
        let waiting = wait_for(&mut child, job.timeout);
        tokio::pin!(waiting);
        tokio::select! {
            result = &mut waiting => {
                let _ = timeout(DRAIN_GRACE, &mut collecting).await;
                result
            }
            _ = &mut collecting => waiting.await,
        }
    };
    RunReport {
        result: alarm_or_failure(result, job.alarm_exit),
        duration: started.elapsed(),
        tail: last_lines(out_ring, err_ring),
    }
}

#[cfg(test)]
mod tests {
    use super::super::jobs::{AUDIT, LESSONS_COMMIT, PROMOTE, USAGE_AUDIT};
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn checkout_with_script(script: &str, body: &str) -> anyhow::Result<tempfile::TempDir> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(script);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        Ok(dir)
    }

    fn promote_in(body: &str) -> anyhow::Result<tempfile::TempDir> {
        checkout_with_script(PROMOTE.script, body)
    }

    fn quick(timeout: Duration) -> Job {
        Job { timeout, ..PROMOTE }
    }

    #[tokio::test]
    async fn a_script_that_exits_zero_succeeds_and_its_output_is_kept() -> anyhow::Result<()> {
        let dir = promote_in("echo promoted 3 skills")?;
        let report = run_job(dir.path(), &quick(Duration::from_secs(30))).await;
        assert_eq!(report.result, RunResult::Succeeded);
        assert_eq!(report.tail, vec!["promoted 3 skills".to_owned()]);
        Ok(())
    }

    #[tokio::test]
    async fn a_script_that_exits_nonzero_reports_the_code_and_its_stderr() -> anyhow::Result<()> {
        let dir = promote_in("echo conflict in memory.md >&2\nexit 3")?;
        let report = run_job(dir.path(), &quick(Duration::from_secs(30))).await;
        assert_eq!(report.result, RunResult::Exited(3));
        assert_eq!(report.tail, vec!["conflict in memory.md".to_owned()]);
        Ok(())
    }

    #[tokio::test]
    async fn only_the_last_lines_of_a_long_output_are_kept() -> anyhow::Result<()> {
        let dir = promote_in("i=1\nwhile [ $i -le 100 ]; do echo line $i; i=$((i+1)); done")?;
        let report = run_job(dir.path(), &quick(Duration::from_secs(30))).await;
        assert_eq!(report.tail.len(), TAIL_LINES);
        assert_eq!(report.tail.first().map(String::as_str), Some("line 81"));
        assert_eq!(report.tail.last().map(String::as_str), Some("line 100"));
        Ok(())
    }

    #[tokio::test]
    async fn a_script_past_its_time_limit_is_stopped() -> anyhow::Result<()> {
        let dir = promote_in("echo started\nexec sleep 30")?;
        let began = Instant::now();
        let limit = Duration::from_millis(500);
        let report = run_job(dir.path(), &quick(limit)).await;
        assert_eq!(report.result, RunResult::TimedOut(limit));
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(report.tail, vec!["started".to_owned()]);
        Ok(())
    }

    #[tokio::test]
    async fn a_checkout_without_the_script_is_unrunnable() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let report = run_job(dir.path(), &quick(Duration::from_secs(30))).await;
        let RunResult::Unrunnable(why) = report.result else {
            anyhow::bail!("expected Unrunnable, got {:?}", report.result);
        };
        assert!(why.contains("scripts/learning-promote"), "{why}");
        Ok(())
    }

    #[tokio::test]
    async fn the_script_runs_in_the_checkout_with_a_private_umask() -> anyhow::Result<()> {
        let dir = promote_in("pwd -P\ntouch made\nls -l made")?;
        let report = run_job(dir.path(), &quick(Duration::from_secs(30))).await;
        assert_eq!(report.result, RunResult::Succeeded);
        let physical = dir.path().canonicalize()?;
        assert_eq!(report.tail.first(), Some(&physical.display().to_string()));
        let listing = report.tail.last().cloned().unwrap_or_default();
        assert!(listing.starts_with("-rw-------"), "{listing}");
        Ok(())
    }

    #[tokio::test]
    async fn a_job_passes_its_arguments_to_the_script() -> anyhow::Result<()> {
        let dir = checkout_with_script(AUDIT.script, "echo \"args: $*\"")?;
        let report = run_job(dir.path(), &AUDIT).await;
        assert_eq!(report.result, RunResult::Succeeded);
        assert_eq!(report.tail, vec!["args: --weekly --sync".to_owned()]);
        Ok(())
    }

    async fn niceness_of(script_dir: &Path, job: &Job) -> anyhow::Result<i32> {
        let report = run_job(script_dir, job).await;
        assert_eq!(report.result, RunResult::Succeeded);
        Ok(report
            .tail
            .first()
            .map_or("", String::as_str)
            .trim()
            .parse()?)
    }

    #[tokio::test]
    async fn each_job_runs_at_its_own_niceness() -> anyhow::Result<()> {
        let dir = checkout_with_script(PROMOTE.script, "ps -o ni= -p $$")?;
        let gentle = niceness_of(dir.path(), &quick(Duration::from_secs(30))).await?;
        let idle = Job {
            nice: 19,
            ..quick(Duration::from_secs(30))
        };
        assert_eq!(niceness_of(dir.path(), &idle).await?, 19);
        assert!(gentle > 0 && gentle <= 19, "{gentle}");
        Ok(())
    }

    #[tokio::test]
    async fn usage_audit_exit_one_is_an_alarm_and_keeps_its_output() -> anyhow::Result<()> {
        let dir = checkout_with_script(
            USAGE_AUDIT.script,
            "echo 'cache re-write rate 3% exceeds 1.5%' >&2\necho 'full report: r.txt' >&2\nexit 1",
        )?;
        let report = run_job(dir.path(), &USAGE_AUDIT).await;
        assert_eq!(report.result, RunResult::Alarmed(1));
        assert!(report.result.completed());
        assert_eq!(
            report.tail,
            vec![
                "cache re-write rate 3% exceeds 1.5%".to_owned(),
                "full report: r.txt".to_owned()
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn usage_audit_other_exit_codes_stay_failures() -> anyhow::Result<()> {
        let dir = checkout_with_script(USAGE_AUDIT.script, "echo no database url >&2\nexit 2")?;
        let report = run_job(dir.path(), &USAGE_AUDIT).await;
        assert_eq!(report.result, RunResult::Exited(2));
        assert!(!report.result.completed());
        Ok(())
    }

    #[tokio::test]
    async fn promote_exit_one_is_an_alarm_for_a_preserved_conflict() -> anyhow::Result<()> {
        let dir = promote_in("echo 'kept hand edit in memory.md as a conflict' >&2\nexit 1")?;
        let report = run_job(dir.path(), &PROMOTE).await;
        assert_eq!(report.result, RunResult::Alarmed(1));
        assert!(report.result.completed());
        assert_eq!(
            report.tail,
            vec!["kept hand edit in memory.md as a conflict".to_owned()]
        );
        Ok(())
    }

    #[tokio::test]
    async fn exit_one_is_a_failure_for_a_job_without_an_alarm_code() -> anyhow::Result<()> {
        let dir = checkout_with_script(LESSONS_COMMIT.script, "exit 1")?;
        let report = run_job(dir.path(), &LESSONS_COMMIT).await;
        assert_eq!(report.result, RunResult::Exited(1));
        Ok(())
    }

    #[test]
    fn exit_codes_exist_only_for_runs_that_exited() {
        assert_eq!(RunResult::Succeeded.exit_code(), Some(0));
        assert_eq!(RunResult::Alarmed(1).exit_code(), Some(1));
        assert_eq!(RunResult::Exited(3).exit_code(), Some(3));
        assert_eq!(RunResult::Signalled.exit_code(), None);
        assert_eq!(RunResult::TimedOut(PROMOTE.timeout).exit_code(), None);
        assert_eq!(RunResult::Unrunnable("x".to_owned()).exit_code(), None);
    }

    #[test]
    fn results_describe_themselves_for_the_status_row() {
        assert_eq!(RunResult::Exited(1).describe(), "exited with code 1");
        assert!(
            RunResult::TimedOut(PROMOTE.timeout)
                .describe()
                .contains("15 minutes")
        );
        assert!(
            RunResult::TimedOut(AUDIT.timeout)
                .describe()
                .contains("30 minutes")
        );
    }
}
