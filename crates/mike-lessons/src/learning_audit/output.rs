use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::report::{PreviousReport, Report};

const LATEST_FILE: &str = "latest.json";
const LATEST_TEMPORARY: &str = ".latest.json.tmp";
const RUN_FOLDER_FORMAT: &str = "%Y-%m-%dT%H%M%SZ";

pub(super) fn read_previous(output: &Path) -> Option<PreviousReport> {
    let bytes = std::fs::read(output.join(LATEST_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn pretty<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)? + "\n")
}

fn write(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))
}

pub(super) fn write_report(
    output: &Path,
    end: DateTime<Utc>,
    report: &Report,
    rendered: &str,
) -> Result<PathBuf> {
    let target = output.join(end.format(RUN_FOLDER_FORMAT).to_string());
    std::fs::create_dir_all(&target)
        .with_context(|| format!("cannot create {}", target.display()))?;
    let full = pretty(report)?;
    write(&target.join("report.json"), &full)?;
    write(
        &target.join("opportunities.json"),
        &pretty(&report.opportunities)?,
    )?;
    write(&target.join("report.md"), rendered)?;
    let temporary = output.join(LATEST_TEMPORARY);
    write(&temporary, &full)?;
    std::fs::rename(&temporary, output.join(LATEST_FILE))
        .with_context(|| format!("cannot replace {}", output.join(LATEST_FILE).display()))?;
    Ok(target.join("report.md"))
}
