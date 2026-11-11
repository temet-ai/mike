use std::collections::BTreeSet;

use chrono::{Duration, Utc};

use super::catalogue::{CHECKS, Probe, due};
use super::daily_pass_due;
use crate::wright::{Cadence, Source};

#[test]
fn every_source_has_at_least_one_check() {
    for source in Source::ALL {
        assert!(
            CHECKS.iter().any(|c| c.source == source),
            "no check reads {}",
            source.code()
        );
    }
}

#[test]
fn check_names_are_unique() {
    let names: BTreeSet<&str> = CHECKS.iter().map(|c| c.name).collect();
    assert_eq!(names.len(), CHECKS.len());
}

#[test]
fn every_compiled_query_is_a_bounded_read() {
    for check in CHECKS {
        let Probe::Sql(sql) = check.probe else {
            continue;
        };
        let trimmed = sql.trim_start().to_ascii_uppercase();
        assert!(
            trimmed.starts_with("WITH") || trimmed.starts_with("SELECT"),
            "{} must be a read",
            check.name
        );
        assert!(
            sql.contains("LIMIT 200"),
            "{} must bound its batch",
            check.name
        );
        assert!(
            sql.contains("interval"),
            "{} must bound its window",
            check.name
        );
    }
    assert!(include_str!("sql/cold_forks.sql").contains("LIMIT 200"));
}

#[test]
fn due_runs_event_checks_on_the_regular_pass_and_slow_checks_daily() {
    let regular: Vec<&str> = due(&[Cadence::Event, Cadence::Regular])
        .map(|c| c.name)
        .collect();
    assert!(regular.contains(&"request_failures"));
    assert!(!regular.contains(&"cold_forks"));
    let event: Vec<&str> = due(&[Cadence::Event]).map(|c| c.name).collect();
    assert_eq!(event, ["request_failures"]);
    let daily: Vec<&str> = due(&[Cadence::Daily]).map(|c| c.name).collect();
    assert_eq!(daily, ["scheduled_job_failures", "cold_forks"]);
}

#[test]
fn daily_pass_due_once_per_utc_day() {
    let now = Utc::now();
    assert!(daily_pass_due(None, now));
    assert!(!daily_pass_due(Some(now), now));
    assert!(daily_pass_due(Some(now - Duration::days(1)), now));
}
