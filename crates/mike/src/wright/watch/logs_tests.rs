use std::io::Write as _;
use std::path::Path;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::logs::{
    LogLine, LogParams, Origin, TAIL_BYTES, findings_with, parse_line, read_tail, shape,
};

fn errors() -> LogParams {
    params(json!({"level": "ERROR", "repeats": 3, "window_minutes": 60}))
}

fn warnings() -> LogParams {
    params(json!({"level": "WARN", "repeats": 100, "window_minutes": 60}))
}

fn params(value: Value) -> LogParams {
    LogParams::from_value(&value).unwrap_or(LogParams {
        level: String::new(),
        repeats: 0,
        window_minutes: 0,
    })
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

fn parsed(at: DateTime<Utc>, level: &str, target: &str, message: &str) -> LogLine {
    LogLine {
        at,
        level: level.to_owned(),
        target: target.to_owned(),
        message: message.to_owned(),
        raw: message.to_owned(),
        origin: None,
    }
}

fn from(machine: Uuid, source: &str, mut line: LogLine) -> LogLine {
    line.origin = Some(Origin {
        machine,
        source: source.to_owned(),
    });
    line
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
fn parse_line_slow_statements_with_summaries_group_by_their_sql() {
    let slow = |summary: &str| {
        json!({
            "timestamp": Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true),
            "level": "WARN",
            "fields": {"message": "slow statement: execution time exceeded alert threshold",
                       "summary": summary},
            "target": "sqlx::query",
        })
        .to_string()
    };
    let first = parse_line(&slow("WITH candidates AS ( SELECT s.source_kind, …"));
    let second = parse_line(&slow("SELECT DISTINCT ON (m.id) m.body …"));
    let (Some(first), Some(second)) = (first, second) else {
        panic!("both slow statement lines must parse");
    };
    assert_eq!(
        first.message,
        "slow statement: execution time exceeded alert threshold: WITH candidates AS ( SELECT s.source_kind, …"
    );
    assert_ne!(shape(&first.message), shape(&second.message));
}

#[test]
fn parse_line_reads_the_json_format_and_skips_pretty_lines() {
    let now = Utc::now();
    let parsed = parse_line(&line(now, "ERROR", "mike::proxy", "boom"));
    assert!(parsed.is_some_and(|l| l.level == "ERROR" && l.target == "mike::proxy"));
    assert!(parse_line("\u{1b}[2m2026-11-11T18:54:59Z\u{1b}[0m INFO hello").is_none());
}

#[tokio::test]
async fn findings_with_an_error_repeated_three_times_in_the_hour_is_one_finding() {
    let now = Utc::now();
    let mut lines: Vec<LogLine> = (0..3)
        .map(|n| {
            parsed(
                now - Duration::minutes(n),
                "ERROR",
                "mike::capture",
                &format!("insert failed after {n} retries"),
            )
        })
        .collect();
    lines.push(parsed(now, "ERROR", "mike::worker", "one-off"));
    lines.push(parsed(
        now - Duration::hours(3),
        "ERROR",
        "mike::capture",
        "insert failed after 7 retries",
    ));
    lines.push(parsed(
        now,
        "WARN",
        "mike::capture",
        "insert failed after 1 retries",
    ));
    let findings = findings_with(None, &lines, now, &errors()).await;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].scope, "log");
    assert_eq!(
        findings[0].resource,
        "mike::capture: insert failed after N retries"
    );
    assert_eq!(findings[0].evidence["lines"], 3);
}

#[tokio::test]
async fn findings_with_a_single_panic_line_is_a_finding() {
    let now = Utc::now();
    let lines = [parsed(
        now,
        "ERROR",
        "mike::serve",
        "task panicked at src/x.rs:1",
    )];
    assert_eq!(findings_with(None, &lines, now, &errors()).await.len(), 1);
}

#[tokio::test]
async fn findings_with_one_target_and_shape_on_two_machines_is_one_finding_naming_both() {
    let now = Utc::now();
    let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
    let lines = [
        from(
            first,
            "mike-json",
            parsed(
                now - Duration::minutes(2),
                "WARN",
                "proxy_forward::ca",
                "new CA 1",
            ),
        ),
        from(
            second,
            "sidecar",
            parsed(
                now - Duration::minutes(1),
                "WARN",
                "proxy_forward::ca",
                "new CA 2",
            ),
        ),
        from(
            second,
            "sidecar",
            parsed(now, "WARN", "proxy_forward::ca", "new CA 3"),
        ),
    ];
    let params = params(json!({"level": "WARN", "repeats": 3, "window_minutes": 60}));
    let findings = findings_with(None, &lines, now, &params).await;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].resource, "proxy_forward::ca: new CA N");
    assert_eq!(findings[0].evidence["lines"], 3);
    let mut machines: Vec<String> = [first, second].iter().map(Uuid::to_string).collect();
    machines.sort();
    assert_eq!(findings[0].evidence["machines"], json!(machines));
    assert_eq!(
        findings[0].evidence["sources"],
        json!(["mike-json", "sidecar"])
    );
}

#[tokio::test]
async fn findings_with_warn_floods_respect_the_repeat_threshold() {
    let now = Utc::now();
    let flood = |count: i64| -> Vec<LogLine> {
        (0..count)
            .map(|n| {
                parsed(
                    now - Duration::seconds(n),
                    "WARN",
                    "mike::launch::fleet::staff",
                    &format!("fleet: ready tasks get no seat (waited {n}s)"),
                )
            })
            .collect()
    };
    let hundred = findings_with(None, &flood(100), now, &warnings()).await;
    assert_eq!(hundred.len(), 1);
    assert_eq!(
        hundred[0].resource,
        "mike::launch::fleet::staff: fleet: ready tasks get no seat (waited Ns)"
    );
    assert_eq!(hundred[0].evidence["lines"], 100);
    assert_eq!(hundred[0].evidence["level"], "WARN");
    assert!(
        findings_with(None, &flood(99), now, &warnings())
            .await
            .is_empty()
    );
    assert!(
        findings_with(None, &flood(99), now, &errors())
            .await
            .is_empty()
    );
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
