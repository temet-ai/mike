use chrono::{TimeZone, Utc};
use serde_json::json;

use super::output::{read_previous, write_report};
use super::report::{Report, Totals};

fn report_with(sessions: i64, requests: i64) -> Report {
    Report {
        version: 1,
        start: "2026-11-02T00:00:00+00:00".to_owned(),
        end: "2026-11-09T00:00:00+00:00".to_owned(),
        totals: Totals {
            sessions,
            requests,
            tool_calls: 0,
            missing_usage: 0,
        },
        opportunities: vec![json!({"tool_name": "Read"})],
        ..Report::default()
    }
}

#[test]
fn write_report_creates_the_run_folder_and_replaces_latest() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let end = Utc
        .with_ymd_and_hms(2026, 11, 9, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid test instant"))?;
    let written = write_report(directory.path(), end, &report_with(4, 10), "# Audit\n")?;
    let folder = directory.path().join("2026-11-09T000000Z");
    assert_eq!(written, folder.join("report.md"));
    assert_eq!(std::fs::read_to_string(&written)?, "# Audit\n");
    let full: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("report.json"))?)?;
    assert_eq!(full["totals"]["requests"], 10);
    let opportunities: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("opportunities.json"))?)?;
    assert_eq!(opportunities, json!([{"tool_name": "Read"}]));
    assert_eq!(
        std::fs::read(directory.path().join("latest.json"))?,
        std::fs::read(folder.join("report.json"))?
    );
    assert!(!directory.path().join(".latest.json.tmp").exists());
    Ok(())
}

#[test]
fn previous_window_reports_the_difference_in_every_total() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    assert!(read_previous(directory.path()).is_none());
    let end = Utc
        .with_ymd_and_hms(2026, 11, 2, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid test instant"))?;
    write_report(directory.path(), end, &report_with(4, 10), "")?;
    let Some(previous) = read_previous(directory.path()) else {
        anyhow::bail!("latest.json was written but not read back");
    };
    let window = previous.against(&report_with(6, 7).totals);
    assert_eq!(window.delta.sessions, 2);
    assert_eq!(window.delta.requests, -3);
    assert_eq!(window.totals.sessions, 4);
    assert_eq!(window.start, "2026-11-02T00:00:00+00:00");
    Ok(())
}

#[test]
fn previous_window_ignores_a_latest_file_that_is_not_a_report() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("latest.json"), b"[]")?;
    assert!(read_previous(directory.path()).is_none());
    Ok(())
}
