use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::introduce::CommandTiming;
use super::work_of::work_through;
use crate::store::persistence::fleet::SlowCommandRow;

pub const DIR_ENV: &str = "MIKE_FLEET_TIMINGS";
const SLACK_SECONDS: i64 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    pub session: Uuid,
    #[serde(default)]
    pub program: Option<String>,
    pub args: String,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub lock_acquired_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub admitted_at: Option<DateTime<Utc>>,
    pub finished_at: DateTime<Utc>,
    pub exit: i32,
}

impl Timing {
    fn seconds(&self) -> f64 {
        (self.finished_at - self.admitted_at.unwrap_or(self.started_at)).as_seconds_f64()
    }

    fn is_work(&self, work: &str) -> bool {
        work == self.args
            || work.split_once(' ').is_some_and(|(program, rest)| {
                rest == self.args && self.program.as_deref().is_none_or(|own| own == program)
            })
    }
}

fn file(dir: &Path, run: &str) -> PathBuf {
    let name: String = run
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{name}.jsonl"))
}

pub fn append(dir: &Path, run: &str, timing: &Timing) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut line = serde_json::to_vec(timing)?;
    line.push(b'\n');
    std::fs::File::options()
        .create(true)
        .append(true)
        .open(file(dir, run))?
        .write_all(&line)
}

pub fn read_run(dir: &Path, run: &str) -> Vec<Timing> {
    std::fs::read_to_string(file(dir, run))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub fn record(
    program: &str,
    args: &[OsString],
    (started_at, admitted_at): (DateTime<Utc>, DateTime<Utc>),
    exit: i32,
) {
    let (Some(dir), Ok(run)) = (std::env::var_os(DIR_ENV), std::env::var("MIKE_RUN_ID")) else {
        return;
    };
    let Some(session) = std::env::var("MIKE_SESSION_ID")
        .ok()
        .and_then(|id| id.parse().ok())
    else {
        return;
    };
    let Some(words) = args.iter().map(|a| a.to_str()).collect::<Option<Vec<_>>>() else {
        return;
    };
    let timing = Timing {
        session,
        program: Some(program.to_string()),
        args: words.join(" "),
        started_at,
        lock_acquired_at: None,
        admitted_at: Some(admitted_at),
        finished_at: Utc::now(),
        exit,
    };
    if let Err(err) = append(Path::new(&dir), &run, &timing) {
        eprintln!("mike: could not record how long {program} took ({err})");
    }
}

fn ran_inside(row: &SlowCommandRow, timing: &Timing) -> bool {
    let slack = Duration::seconds(SLACK_SECONDS);
    let ends_by = if row.in_background {
        timing.started_at
    } else {
        timing.finished_at
    };
    timing.session == row.session
        && timing.started_at >= row.started_at - slack
        && ends_by <= row.finished_at + slack
}

fn measured(row: &SlowCommandRow, timing: &Timing) -> SlowCommandRow {
    if !row.in_background {
        return row.clone();
    }
    SlowCommandRow {
        in_background: false,
        finished_at: timing.finished_at,
        is_error: timing.exit != 0,
        ..row.clone()
    }
}

pub fn matched(
    rows: &[SlowCommandRow],
    recorded: &[Timing],
    wrappers: &[String],
) -> (Vec<SlowCommandRow>, Vec<CommandTiming>) {
    let mut out_rows = Vec::with_capacity(rows.len());
    let mut timings = Vec::new();
    for row in rows {
        let works = work_through(&row.command, wrappers);
        let found: Vec<(&Timing, &String)> = recorded
            .iter()
            .filter(|timing| ran_inside(row, timing))
            .filter_map(|timing| {
                works
                    .iter()
                    .find(|w| timing.is_work(w))
                    .map(|w| (timing, w))
            })
            .collect();
        let single_background = !row.in_background || found.len() == 1;
        match found.first() {
            Some((timing, _)) if single_background => out_rows.push(measured(row, timing)),
            _ => out_rows.push(row.clone()),
        }
        if single_background {
            timings.extend(found.into_iter().map(|(timing, work)| CommandTiming {
                use_id: row.use_id,
                work: work.clone(),
                seconds: timing.seconds(),
            }));
        }
    }
    (out_rows, timings)
}

#[cfg(test)]
#[path = "build_timing_tests.rs"]
mod tests;
