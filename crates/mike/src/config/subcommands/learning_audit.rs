use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, Duration, NaiveTime, Utc};
use clap::Args;

use crate::config::CliOverrides;
use crate::learning_audit::{AuditRequest, run};
use crate::store::persistence::PersistHandle;

const LOCK_FILE: &str = ".local/state/mike-learning-audit.lock";
const PROMOTION_STATE: &str = ".local/state/mike-learning-promotion";
const TOOL_REUSE_STATE: &str = ".local/state/mike-tool-reuse";
const TOOL_REUSE_STATE_VARIABLE: &str = "MIKE_TOOL_REUSE_STATE";
const INSTALLED_REPORTS: &str = ".local/state/mike/learning/artifacts/learning-audit";
const CHECKOUT_REPORTS: &str = "artifacts/learning-audit";
const WRAPPER: &str = "scripts/learning-audit";

#[derive(Debug, Clone, Args)]
pub struct LearningAuditArgs {
    #[arg(
        long,
        conflicts_with = "end",
        help = "Use the latest Monday 00:00 UTC as the exclusive boundary"
    )]
    pub weekly: bool,
    #[arg(
        long,
        help = "Exclusive RFC 3339 timestamp with a timezone, such as 2026-11-09T00:00:00Z; defaults to now"
    )]
    pub end: Option<String>,
    #[arg(
        long,
        help = "Report folder; defaults to artifacts/learning-audit in a checkout, else the state folder under ~/.local/state/mike/learning"
    )]
    pub output: Option<PathBuf>,
    #[arg(
        long,
        help = "Accepted for the scheduled job and ignored: the report is only written under --output"
    )]
    pub sync: bool,
}

fn weekly_boundary(now: DateTime<Utc>) -> DateTime<Utc> {
    let monday = now.date_naive() - Duration::days(i64::from(now.weekday().num_days_from_monday()));
    monday.and_time(NaiveTime::MIN).and_utc()
}

fn audit_end(args: &LearningAuditArgs, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    if args.weekly {
        return Ok(weekly_boundary(now));
    }
    let Some(value) = args.end.as_deref() else {
        return Ok(now);
    };
    DateTime::parse_from_rfc3339(value)
        .map(|moment| moment.with_timezone(&Utc))
        .with_context(|| {
            format!("--end must be an RFC 3339 timestamp with a timezone, such as 2026-11-09T00:00:00Z; got {value:?}")
        })
}

fn is_installed_layout(root: &Path) -> bool {
    root.file_name().is_some_and(|name| name == "mike")
        && root
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "share")
}

fn default_output(candidates: &[PathBuf], home: &Path) -> PathBuf {
    let found = candidates
        .iter()
        .find(|directory| directory.join(WRAPPER).is_file());
    match found {
        Some(root) if !is_installed_layout(root) => root.join(CHECKOUT_REPORTS),
        _ => home.join(INSTALLED_REPORTS),
    }
}

fn search_roots() -> Vec<PathBuf> {
    let from_cwd = std::env::current_dir().ok();
    let from_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| std::fs::canonicalize(exe).ok());
    from_cwd
        .iter()
        .flat_map(|path| path.ancestors())
        .chain(from_exe.iter().flat_map(|path| path.ancestors()))
        .map(Path::to_path_buf)
        .collect()
}

fn home_dir() -> Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .context("cannot resolve the home folder")
}

fn tool_reuse_dir(home: &Path) -> PathBuf {
    std::env::var_os(TOOL_REUSE_STATE_VARIABLE)
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(TOOL_REUSE_STATE))
}

fn acquire_lock(home: &Path) -> Result<File> {
    let path = home.join(LOCK_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => bail!(
            "another learning audit holds {}; run this one when it finishes",
            path.display()
        ),
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("cannot lock {}", path.display()))
        }
    }
}

async fn connect(cli: CliOverrides) -> Result<PersistHandle> {
    let config =
        crate::config::load::load(cli).context("failed to resolve config for learning-audit")?;
    let Some(database_url) = config.database_url.as_deref() else {
        bail!("learning-audit requires a database_url (set it in /settings or MIKE_DATABASE_URL)");
    };
    PersistHandle::connect_read_only(database_url)
        .await
        .context("learning-audit: could not connect to the database")
}

pub(super) async fn learning_audit(args: LearningAuditArgs, cli: CliOverrides) -> Result<()> {
    let home = home_dir()?;
    let _lock = acquire_lock(&home)?;
    let request = AuditRequest {
        end: audit_end(&args, Utc::now())?,
        output: args
            .output
            .clone()
            .unwrap_or_else(|| default_output(&search_roots(), &home)),
        promotion_dir: home.join(PROMOTION_STATE),
        tool_reuse_dir: tool_reuse_dir(&home),
    };
    let handle = connect(cli).await?;
    let summary = run(&handle, &request).await?;
    println!(
        "{}",
        serde_json::json!({
            "report": summary.report,
            "findings": summary.findings,
            "reuse_candidates": summary.reuse_candidates,
        })
    );
    Ok(())
}

#[cfg(test)]
#[path = "learning_audit_tests.rs"]
mod learning_audit_tests;
