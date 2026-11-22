use std::collections::{BTreeMap, BTreeSet};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use uuid::Uuid;

use super::logs_jev::{Groups, POINT, merge};
use crate::llm::decisions::point::Decider;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::LogLineRow;
use crate::wright::Finding;
use crate::wright::definition::int_param;

pub(crate) const TAIL_BYTES: u64 = 4 * 1024 * 1024;
const LEVELS: [&str; 3] = ["ERROR", "WARN", "INFO"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogParams {
    pub(crate) level: String,
    pub(crate) repeats: usize,
    pub(crate) window_minutes: i64,
}

impl LogParams {
    pub(crate) fn from_value(params: &Value) -> Result<Self, String> {
        let level = params
            .get("level")
            .and_then(Value::as_str)
            .filter(|level| LEVELS.contains(level))
            .ok_or_else(|| {
                format!(
                    "params.level is {}; it must be one of {}",
                    params.get("level").unwrap_or(&Value::Null),
                    LEVELS.join(", ")
                )
            })?;
        let repeats = int_param(params, "repeats", 1, 1_000_000)?;
        Ok(Self {
            level: level.to_owned(),
            repeats: usize::try_from(repeats).map_err(|e| e.to_string())?,
            window_minutes: int_param(params, "window_minutes", 1, 1440)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Origin {
    pub(crate) machine: Uuid,
    pub(crate) source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogLine {
    pub(crate) at: DateTime<Utc>,
    pub(crate) level: String,
    pub(crate) target: String,
    pub(crate) message: String,
    pub(crate) raw: String,
    pub(crate) origin: Option<Origin>,
}

impl From<LogLineRow> for LogLine {
    fn from(row: LogLineRow) -> Self {
        Self {
            at: row.at,
            level: row.level,
            target: row.target,
            message: row.message,
            raw: row.raw,
            origin: Some(Origin {
                machine: row.machine_uuid,
                source: row.source,
            }),
        }
    }
}

pub(crate) fn shape(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut digits = false;
    for ch in message.lines().next().unwrap_or("").chars() {
        if ch.is_ascii_digit() {
            if !digits {
                out.push('N');
            }
            digits = true;
        } else {
            digits = false;
            out.push(ch);
        }
    }
    out.chars().take(160).collect()
}

pub(crate) fn parse_line(raw: &str) -> Option<LogLine> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let at = value.get("timestamp")?.as_str()?.parse().ok()?;
    let message = value
        .pointer("/fields/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    let message = match value.pointer("/fields/summary").and_then(Value::as_str) {
        Some(summary) => format!("{message}: {summary}"),
        None => message.to_owned(),
    };
    Some(LogLine {
        at,
        level: value.get("level")?.as_str()?.to_owned(),
        target: value
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        message,
        raw: raw.to_owned(),
        origin: None,
    })
}

fn is_panic(line: &LogLine) -> bool {
    line.message.contains("panicked")
}

/// sqlx's slow-statement lines are left to the `slow_queries` check, which names each slow
/// query once by its pg_stat_statements queryid.
fn is_slow_statement(line: &LogLine) -> bool {
    line.target == "sqlx::query" && line.message.starts_with("slow statement:")
}

pub(crate) fn group_lines<'a>(
    lines: &'a [LogLine],
    now: DateTime<Utc>,
    params: &LogParams,
) -> Groups<'a> {
    let since = now - Duration::minutes(params.window_minutes);
    let mut grouped: Groups<'a> = BTreeMap::new();
    for line in lines
        .iter()
        .filter(|l| l.level == params.level && l.at >= since && !is_slow_statement(l))
    {
        grouped
            .entry((line.target.clone(), shape(&line.message)))
            .or_default()
            .push(line);
    }
    grouped
}

pub(crate) async fn findings_with(
    decider: Option<&Decider>,
    lines: &[LogLine],
    now: DateTime<Utc>,
    params: &LogParams,
) -> Vec<Finding> {
    let grouped = group_lines(lines, now, params);
    match decider {
        Some(decider) => findings_of(merge(decider, grouped, params.repeats).await, params),
        None => findings_of(grouped, params),
    }
}

fn origins(hits: &[&LogLine]) -> (BTreeSet<String>, BTreeSet<Uuid>) {
    let mut sources = BTreeSet::new();
    let mut machines = BTreeSet::new();
    for origin in hits.iter().filter_map(|l| l.origin.as_ref()) {
        sources.insert(origin.source.clone());
        machines.insert(origin.machine);
    }
    (sources, machines)
}

fn findings_of(grouped: Groups<'_>, params: &LogParams) -> Vec<Finding> {
    grouped
        .into_iter()
        .filter(|(_, hits)| hits.len() >= params.repeats || hits.iter().any(|l| is_panic(l)))
        .map(|((target, message), hits)| {
            let (sources, machines) = origins(&hits);
            Finding {
                scope: "log".to_owned(),
                resource: format!("{target}: {message}"),
                evidence: json!({
                    "level": params.level,
                    "window_minutes": params.window_minutes,
                    "sources": sources,
                    "machines": machines,
                    "lines": hits.len(),
                    "first_at": hits.first().map(|l| l.at),
                    "last_at": hits.last().map(|l| l.at),
                    "sample_line": hits.last().map(|l| l.raw.clone()),
                }),
            }
        })
        .collect()
}

pub(crate) async fn newest_log(dir: &Path) -> Result<PathBuf, String> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| format!("cannot read log folder {}: {e}", dir.display()))?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("cannot list log folder {}: {e}", dir.display()))?
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("mike.") && name.ends_with(".log")) {
            continue;
        }
        let Ok(modified) = entry.metadata().await.and_then(|m| m.modified()) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(at, _)| modified > *at) {
            newest = Some((modified, entry.path()));
        }
    }
    newest
        .map(|(_, path)| path)
        .ok_or_else(|| format!("no mike.*.log file under {}", dir.display()))
}

pub(crate) async fn read_tail(path: &Path) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let len = file
        .metadata()
        .await
        .map_err(|e| format!("cannot stat {}: {e}", path.display()))?
        .len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))
        .await
        .map_err(|e| format!("cannot seek {}: {e}", path.display()))?;
    let mut bytes = Vec::with_capacity(usize::try_from(len - start).unwrap_or(0));
    file.read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let whole_lines = if start > 0 {
        text.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &text
    };
    Ok(whole_lines.to_owned())
}

pub(crate) async fn check(
    persist: &PersistHandle,
    now: DateTime<Utc>,
    params: &Value,
) -> Result<Vec<Finding>, String> {
    let params = LogParams::from_value(params)?;
    let since = now - Duration::minutes(params.window_minutes);
    let rows = persist
        .web()
        .recent_wright_log_lines(since, &params.level)
        .await
        .map_err(|e| format!("cannot read wright.log_lines: {e}"))?;
    let lines: Vec<LogLine> = rows.into_iter().map(LogLine::from).collect();
    let decider = crate::llm::decisions::ambient::decider(POINT);
    Ok(findings_with(decider.as_ref(), &lines, now, &params).await)
}
