use std::collections::BTreeMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::logs_jev::{Groups, POINT, merge};
use crate::llm::decisions::point::Decider;
use crate::wright::Finding;
use crate::wright::definition::int_param;

pub(crate) const TAIL_BYTES: u64 = 4 * 1024 * 1024;
const STALE_HOURS: i64 = 48;
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogLine {
    pub(crate) at: DateTime<Utc>,
    pub(crate) level: String,
    pub(crate) target: String,
    pub(crate) message: String,
    pub(crate) raw: String,
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
    Some(LogLine {
        at,
        level: value.get("level")?.as_str()?.to_owned(),
        target: value
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        message: message.to_owned(),
        raw: raw.to_owned(),
    })
}

fn is_panic(line: &LogLine) -> bool {
    line.message.contains("panicked")
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
        .filter(|l| l.level == params.level && l.at >= since)
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
    (file, params): (&Path, &LogParams),
) -> Vec<Finding> {
    let grouped = group_lines(lines, now, params);
    match decider {
        Some(decider) => findings_of(merge(decider, grouped, params.repeats).await, file, params),
        None => findings_of(grouped, file, params),
    }
}

fn findings_of(grouped: Groups<'_>, file: &Path, params: &LogParams) -> Vec<Finding> {
    grouped
        .into_iter()
        .filter(|(_, hits)| hits.len() >= params.repeats || hits.iter().any(|l| is_panic(l)))
        .map(|((target, message), hits)| Finding {
            scope: "log".to_owned(),
            resource: format!("{target}: {message}"),
            evidence: json!({
                "level": params.level,
                "window_minutes": params.window_minutes,
                "file": file.display().to_string(),
                "lines": hits.len(),
                "first_at": hits.first().map(|l| l.at),
                "last_at": hits.last().map(|l| l.at),
                "sample_line": hits.last().map(|l| l.raw.clone()),
            }),
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
    dir: Option<&Path>,
    now: DateTime<Utc>,
    params: &Value,
) -> Result<Vec<Finding>, String> {
    let params = LogParams::from_value(params)?;
    let dir = dir.ok_or_else(|| "no log folder is configured (log_dir)".to_owned())?;
    let file = newest_log(dir).await?;
    let text = read_tail(&file).await?;
    let lines: Vec<LogLine> = text.lines().filter_map(parse_line).collect();
    let newest = lines
        .iter()
        .map(|l| l.at)
        .max()
        .ok_or_else(|| format!("{} holds no readable log line", file.display()))?;
    if now - newest > Duration::hours(STALE_HOURS) {
        return Err(format!(
            "{} has nothing newer than {newest}; the log is not being written",
            file.display()
        ));
    }
    let decider = crate::llm::decisions::ambient::decider(POINT);
    Ok(findings_with(decider.as_ref(), &lines, now, (&file, &params)).await)
}
