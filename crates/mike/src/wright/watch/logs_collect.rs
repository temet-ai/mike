use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::catalogue::WatchContext;
use super::logs::{LogLine, newest_log, parse_line, read_tail};
use super::logs_text::{Keep, parse_text};
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::NewLogLine;
use crate::wright::repair::RepoLayout;
use crate::wright::repair::runner::ProcessRunner;

pub(crate) const MIKE_JSON: &str = "mike-json";
pub(crate) const SIDECAR: &str = "sidecar";
pub(crate) const PROXY_STDOUT: &str = "proxy-stdout";
const WINDOW_MINUTES: i64 = 60;
const KEEP_HOURS: i64 = 2;
const COLLECTED_LEVELS: [&str; 2] = ["ERROR", "WARN"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Json,
    Text(Keep),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) path: PathBuf,
    pub(crate) label: &'static str,
    pub(crate) format: Format,
}

pub(crate) async fn discover_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    match RepoLayout::discover(&ProcessRunner, &cwd).await {
        Ok(repo) => Some(repo.root),
        Err(e) => {
            tracing::debug!(error = %e, cwd = %cwd.display(), "wright: no checkout, its .deploy logs are not read");
            None
        }
    }
}

pub(crate) fn text_sources(log_dir: Option<&Path>, root: Option<&Path>) -> Vec<Source> {
    let deploy = root.map(|root| root.join(".deploy"));
    let candidates = [
        (log_dir, "sidecar.log", SIDECAR, Format::Text(Keep::Levels)),
        (
            log_dir,
            "proxy.log",
            PROXY_STDOUT,
            Format::Text(Keep::PanicsOnly),
        ),
        (
            deploy.as_deref(),
            "proxy-forward.log",
            SIDECAR,
            Format::Text(Keep::Levels),
        ),
        (
            deploy.as_deref(),
            "mike.log",
            PROXY_STDOUT,
            Format::Text(Keep::PanicsOnly),
        ),
    ];
    let mut out: Vec<Source> = Vec::new();
    for (dir, name, label, format) in candidates {
        let Some(dir) = dir else { continue };
        let path = dir.join(name);
        if out.iter().all(|s| s.path != path) {
            out.push(Source {
                path,
                label,
                format,
            });
        }
    }
    out
}

async fn sources(log_dir: Option<&Path>, root: Option<&Path>) -> Vec<Source> {
    let mut out = Vec::new();
    if let Some(dir) = log_dir {
        match newest_log(dir).await {
            Ok(path) => out.push(Source {
                path,
                label: MIKE_JSON,
                format: Format::Json,
            }),
            Err(reason) => tracing::warn!(reason, "wright: no JSON log to collect"),
        }
    }
    out.extend(text_sources(log_dir, root));
    out
}

pub(crate) fn parse_source(text: &str, format: Format) -> Vec<LogLine> {
    match format {
        Format::Json => text
            .lines()
            .filter_map(parse_line)
            .filter(|l| COLLECTED_LEVELS.contains(&l.level.as_str()))
            .collect(),
        Format::Text(keep) => parse_text(text, keep),
    }
}

async fn read_source(source: &Source, since: DateTime<Utc>) -> Vec<NewLogLine> {
    if tokio::fs::metadata(&source.path).await.is_err() {
        return Vec::new();
    }
    match read_tail(&source.path).await {
        Ok(text) => parse_source(&text, source.format)
            .into_iter()
            .filter(|line| line.at >= since)
            .map(|line| stored(source.label, line))
            .collect(),
        Err(reason) => {
            tracing::warn!(reason, source = source.label, "wright: cannot read a log");
            Vec::new()
        }
    }
}

fn stored(source: &str, line: LogLine) -> NewLogLine {
    let text = |text: &str| -> String { text.chars().filter(|c| *c != '\0').collect() };
    NewLogLine {
        source: source.to_owned(),
        at: line.at,
        level: line.level,
        target: text(&line.target),
        message: text(&line.message),
        raw: text(&line.raw),
    }
}

pub(crate) async fn collect(ctx: &WatchContext, now: DateTime<Utc>) {
    let Some(machine) = ctx.machine else { return };
    let fresh = gather(ctx.log_dir.as_deref(), ctx.repo_root.as_deref(), now).await;
    store_new(ctx, machine, fresh, now).await;
}

pub(crate) async fn gather(
    log_dir: Option<&Path>,
    root: Option<&Path>,
    now: DateTime<Utc>,
) -> Vec<NewLogLine> {
    let since = now - Duration::minutes(WINDOW_MINUTES);
    let mut fresh = Vec::new();
    for source in sources(log_dir, root).await {
        fresh.extend(read_source(&source, since).await);
    }
    fresh
}

async fn store_new(ctx: &WatchContext, machine: Uuid, fresh: Vec<NewLogLine>, now: DateTime<Utc>) {
    let newest = match ctx.persist.web().wright_log_newest(machine).await {
        Ok(newest) => newest,
        Err(e) => {
            return tracing::warn!(error = %e, "wright: cannot read the newest stored log lines");
        }
    };
    let new: Vec<NewLogLine> = fresh
        .into_iter()
        .filter(|l| newest.get(&l.source).is_none_or(|at| l.at > *at))
        .collect();
    let keep_since = now - Duration::hours(KEEP_HOURS);
    match ctx
        .persist
        .web()
        .store_wright_log_lines(machine, &new, keep_since)
        .await
    {
        Ok(stored) => tracing::debug!(stored, "wright: collected log lines"),
        Err(e) => tracing::warn!(error = %e, "wright: cannot store log lines"),
    }
}
