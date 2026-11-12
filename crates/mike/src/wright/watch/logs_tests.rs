use std::io::Write as _;
use std::path::Path;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{Value, json};

use super::logs::{LogParams, TAIL_BYTES, check, parse_line, read_tail, shape};

fn errors() -> Value {
    json!({"level": "ERROR", "repeats": 3, "window_minutes": 60})
}

fn line(at: DateTime<Utc>, level: &str, target: &str, message: &str) -> String {
    json!({
        "timestamp": at.to_rfc3339_opts(SecondsFormat::Micros, true),
        "level": level,
        "fields": {"message": message},
        "target": target,
    })
    .to_string()
}

fn write_log(dir: &Path, name: &str, lines: &[String]) -> anyhow::Result<()> {
    let mut file = std::fs::File::create(dir.join(name))?;
    for l in lines {
        writeln!(file, "{l}")?;
    }
    Ok(())
}

#[test]
fn shape_replaces_numbers_and_keeps_the_first_line() {
    assert_eq!(shape("timeout after 3000 ms\nmore"), "timeout after N ms");
}

#[test]
fn parse_line_reads_the_json_format_and_skips_pretty_lines() {
    let now = Utc::now();
    let parsed = parse_line(&line(now, "ERROR", "mike::proxy", "boom"));
    assert!(parsed.is_some_and(|l| l.level == "ERROR" && l.target == "mike::proxy"));
    assert!(parse_line("\u{1b}[2m2026-11-11T18:54:59Z\u{1b}[0m INFO hello").is_none());
}

#[tokio::test]
async fn check_finds_an_error_repeated_three_times_within_the_hour() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let now = Utc::now();
    let mut lines = Vec::new();
    for n in 0..3 {
        lines.push(line(
            now - Duration::minutes(n),
            "ERROR",
            "mike::capture",
            &format!("insert failed after {n} retries"),
        ));
    }
    lines.push(line(
        now - Duration::minutes(5),
        "ERROR",
        "mike::worker",
        "one-off",
    ));
    lines.push(line(
        now - Duration::hours(3),
        "ERROR",
        "mike::capture",
        "insert failed after 7 retries",
    ));
    lines.push(line(
        now - Duration::minutes(2),
        "WARN",
        "mike::capture",
        "insert failed after 1 retries",
    ));
    write_log(dir.path(), "mike.2026-11-11.log", &lines)?;

    let findings = check(Some(dir.path()), now, &errors())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].scope, "log");
    assert_eq!(
        findings[0].resource,
        "mike::capture: insert failed after N retries"
    );
    assert_eq!(findings[0].evidence["lines"], 3);
    Ok(())
}

#[tokio::test]
async fn check_reports_a_single_panic() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let now = Utc::now();
    write_log(
        dir.path(),
        "mike.2026-11-11.log",
        &[line(
            now,
            "ERROR",
            "mike::serve",
            "task panicked at src/x.rs:1",
        )],
    )?;
    let findings = check(Some(dir.path()), now, &errors())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(findings.len(), 1);
    Ok(())
}

#[tokio::test]
async fn check_without_evidence_is_unknown_never_healthy() -> anyhow::Result<()> {
    let now = Utc::now();
    let params = errors();
    assert!(check(None, now, &params).await.is_err());
    let empty = tempfile::tempdir()?;
    assert!(check(Some(empty.path()), now, &params).await.is_err());
    assert!(
        check(Some(&empty.path().join("missing")), now, &params)
            .await
            .is_err()
    );

    write_log(
        empty.path(),
        "mike.2026-11-01.log",
        &[line(now - Duration::days(5), "INFO", "mike", "old")],
    )?;
    let stale = check(Some(empty.path()), now, &params).await;
    assert!(stale.is_err_and(|reason| reason.contains("not being written")));

    let garbled = tempfile::tempdir()?;
    write_log(
        garbled.path(),
        "mike.2026-11-11.log",
        &["not json".to_owned()],
    )?;
    assert!(check(Some(garbled.path()), now, &params).await.is_err());
    Ok(())
}

fn warnings() -> Value {
    json!({"level": "WARN", "repeats": 100, "window_minutes": 60})
}

fn flood(now: DateTime<Utc>, count: i64) -> Vec<String> {
    (0..count)
        .map(|n| {
            line(
                now - Duration::seconds(n),
                "WARN",
                "mike::launch::fleet::staff",
                &format!("fleet: ready tasks get no seat (waited {n}s)"),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_warn_flood_of_one_hundred_lines_in_an_hour_is_a_finding() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let now = Utc::now();
    write_log(dir.path(), "mike.2026-11-12.log", &flood(now, 100))?;
    let findings = check(Some(dir.path()), now, &warnings())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].resource,
        "mike::launch::fleet::staff: fleet: ready tasks get no seat (waited Ns)"
    );
    assert_eq!(findings[0].evidence["lines"], 100);
    assert_eq!(findings[0].evidence["level"], "WARN");
    Ok(())
}

#[tokio::test]
async fn ninety_nine_warn_lines_in_an_hour_are_not_a_finding() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let now = Utc::now();
    write_log(dir.path(), "mike.2026-11-12.log", &flood(now, 99))?;
    let findings = check(Some(dir.path()), now, &warnings())
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(findings.is_empty());
    let errors_only = check(Some(dir.path()), now, &errors())
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(errors_only.is_empty());
    Ok(())
}

#[test]
fn log_params_name_the_offending_value() {
    let level =
        LogParams::from_value(&json!({"level": "TRACE", "repeats": 3, "window_minutes": 60}));
    assert!(level.is_err_and(|e| e.contains("\"TRACE\"") && e.contains("ERROR, WARN, INFO")));
    let window =
        LogParams::from_value(&json!({"level": "WARN", "repeats": 3, "window_minutes": 1441}));
    assert!(window.is_err_and(|e| e.contains("window_minutes") && e.contains("1441")));
    let repeats =
        LogParams::from_value(&json!({"level": "WARN", "repeats": 0, "window_minutes": 60}));
    assert!(repeats.is_err_and(|e| e.contains("repeats")));
}

#[tokio::test]
async fn read_tail_reads_only_the_last_bounded_bytes_from_a_line_start() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("mike.big.log");
    let filler = "x".repeat(1023);
    let count = usize::try_from(TAIL_BYTES / 1024)? + 10;
    let lines: Vec<String> = (0..count).map(|_| filler.clone()).collect();
    write_log(dir.path(), "mike.big.log", &lines)?;
    let tail = read_tail(&path).await.map_err(anyhow::Error::msg)?;
    assert!(u64::try_from(tail.len())? <= TAIL_BYTES);
    assert!(tail.lines().all(|l| l == filler));
    Ok(())
}
