use chrono::{DateTime, Utc};

use super::build_report;
use crate::steerer::structural::wiring_store::StoredFinding;

fn at(text: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(text)?.with_timezone(&Utc))
}

fn stored(subject: &str, resolved_at: Option<DateTime<Utc>>) -> anyhow::Result<StoredFinding> {
    Ok(StoredFinding {
        id: 1,
        check_kind: "toggle_off".to_owned(),
        subject: subject.to_owned(),
        figure_value: 28.0,
        figure_text: "off for 28 days".to_owned(),
        first_seen: at("2026-11-01T00:00:00Z")?,
        last_seen: at("2026-11-14T00:00:00Z")?,
        figure_changed_at: at("2026-11-01T00:00:00Z")?,
        resolved_at,
    })
}

#[test]
fn report_counts_open_and_resolved_findings_and_ages_them() -> anyhow::Result<()> {
    let now = at("2026-11-14T12:00:00Z")?;
    let report = build_report(
        vec![
            stored("toggle:a", None)?,
            stored("toggle:b", Some(at("2026-11-05T00:00:00Z")?))?,
        ],
        now,
        None,
    );
    assert_eq!(report.open, 1);
    assert_eq!(report.resolved, 1);
    assert_eq!(report.findings[0].status, "open");
    assert_eq!(report.findings[0].age_days, 13);
    assert_eq!(report.findings[1].status, "resolved");
    assert_eq!(report.findings[1].age_days, 4);
    Ok(())
}

#[test]
fn report_serialises_the_finding_fields_beside_status_and_age() -> anyhow::Result<()> {
    let now = at("2026-11-14T12:00:00Z")?;
    let report = build_report(vec![stored("toggle:a", None)?], now, Some(now));
    let json = serde_json::to_value(&report)?;
    assert_eq!(json["findings"][0]["subject"], "toggle:a");
    assert_eq!(json["findings"][0]["status"], "open");
    assert_eq!(json["findings"][0]["figure_value"], 28.0);
    assert_eq!(json["open"], 1);
    Ok(())
}
