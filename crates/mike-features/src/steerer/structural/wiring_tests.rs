use chrono::{DateTime, Utc};

use super::never_fired::NeverFiredFinding;
use super::wiring::{
    CoverageRow, Finding, JobRow, PassOutcome, RetirementRow, ShadowQuestionRow, StageRow,
    SteererActivityRow, ToggleRow, UnauditedFleet, coverage_findings, dormant_findings,
    fleet_event_findings, job_findings, retirement_findings, shadow_findings, stage_findings,
    toggle_findings, unaudited_fleet_findings,
};

fn at(text: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(text)?.with_timezone(&Utc))
}

fn stage(name: &str, seen: i64, applied: i64, reason: &str) -> StageRow {
    StageRow {
        stage: name.to_owned(),
        seen,
        applied,
        top_reason: Some(reason.to_owned()),
    }
}

#[test]
fn stage_declined_on_every_request_is_a_finding_with_its_reason() -> anyhow::Result<()> {
    let findings = stage_findings(&[stage("recall_injector", 2000, 0, "recall_no_tags")]);
    let [finding] = findings.as_slice() else {
        anyhow::bail!("expected one finding, got {findings:?}");
    };
    assert_eq!(finding.subject, "stage:recall_injector");
    assert_eq!(finding.figure_value, 0.0);
    assert!(finding.figure_text.contains("recall_no_tags"));
    assert!(finding.figure_text.contains("0 of 2000"));
    Ok(())
}

#[test]
fn stage_applied_often_enough_or_seen_too_rarely_or_served_by_client_is_not_a_finding() {
    let rows = [
        stage("code_findings", 1183, 5, "code_findings_nothing_new"),
        stage("composer", 26, 0, "composer_unknown_shape"),
        stage("git_read_hint", 1399, 0, "git_read_hint_served_by_client"),
    ];
    assert!(stage_findings(&rows).is_empty());
}

#[test]
fn stage_applied_below_a_tenth_of_a_percent_stays_a_finding_with_its_rate() -> anyhow::Result<()> {
    let findings = stage_findings(&[stage("file_read_compression", 2000, 1, "no_candidates")]);
    let [finding] = findings.as_slice() else {
        anyhow::bail!("expected one finding, got {findings:?}");
    };
    assert!((finding.figure_value - 0.05).abs() < 1e-9);
    Ok(())
}

#[test]
fn fleet_event_kinds_are_judged_only_when_the_fleet_was_active() {
    let quiet = NeverFiredFinding {
        rule_name: "fleet event kind",
        never_observed: vec!["seat_woken"],
        total_observations: 3,
    };
    assert_eq!(fleet_event_findings(&quiet), None);
}

#[test]
fn fleet_event_kind_with_no_rows_in_an_active_week_is_a_finding() -> anyhow::Result<()> {
    let active = NeverFiredFinding {
        rule_name: "fleet event kind",
        never_observed: vec!["seat_woken", "floor_passed"],
        total_observations: 400,
    };
    let findings = fleet_event_findings(&active).ok_or_else(|| anyhow::anyhow!("no verdict"))?;
    let subjects: Vec<&str> = findings.iter().map(|f| f.subject.as_str()).collect();
    assert_eq!(subjects, vec!["event:seat_woken", "event:floor_passed"]);
    Ok(())
}

#[test]
fn toggle_off_longer_than_fourteen_days_is_a_finding_with_its_age() -> anyhow::Result<()> {
    let now = at("2026-11-14T12:00:00Z")?;
    let rows = [
        ToggleRow {
            key: "db_resync".to_owned(),
            operator_set: true,
            updated_at: at("2026-10-16T21:26:04Z")?,
        },
        ToggleRow {
            key: "fresh".to_owned(),
            operator_set: true,
            updated_at: at("2026-11-10T00:00:00Z")?,
        },
    ];
    let findings = toggle_findings(&rows, now);
    let [finding] = findings.as_slice() else {
        anyhow::bail!("expected one finding, got {findings:?}");
    };
    assert_eq!(finding.subject, "toggle:db_resync");
    assert_eq!(finding.figure_value, 28.0);
    Ok(())
}

#[test]
fn shadow_question_and_shadow_only_steerer_are_findings_but_a_promoted_steerer_is_not()
-> anyhow::Result<()> {
    let now = at("2026-11-14T12:00:00Z")?;
    let questions = [ShadowQuestionRow {
        steerer: "turn-guard".to_owned(),
        question: "relapse".to_owned(),
        updated_at: at("2026-10-16T08:20:50Z")?,
    }];
    let steerers = [
        SteererActivityRow {
            name: "turn-guard".to_owned(),
            shadow: 52052,
            live: 22862,
            first_shadow: Some(at("2026-09-29T10:53:00Z")?),
            last_evaluation: Some(now),
        },
        SteererActivityRow {
            name: "candidate".to_owned(),
            shadow: 40,
            live: 0,
            first_shadow: Some(at("2026-10-01T00:00:00Z")?),
            last_evaluation: Some(now),
        },
    ];
    let findings = shadow_findings(&questions, &steerers, now);
    let subjects: Vec<&str> = findings.iter().map(|f| f.subject.as_str()).collect();
    assert_eq!(
        subjects,
        vec!["question:turn-guard/relapse", "steerer:candidate"]
    );
    Ok(())
}

#[test]
fn steerer_without_an_evaluation_for_two_weeks_is_dormant() -> anyhow::Result<()> {
    let now = at("2026-11-14T12:00:00Z")?;
    let steerers = [
        SteererActivityRow {
            name: "verifier".to_owned(),
            shadow: 0,
            live: 5982,
            first_shadow: None,
            last_evaluation: Some(at("2026-10-08T13:18:34Z")?),
        },
        SteererActivityRow {
            name: "monitor-over-sleep".to_owned(),
            shadow: 0,
            live: 1838,
            first_shadow: None,
            last_evaluation: Some(now),
        },
    ];
    let findings = dormant_findings(&steerers, now);
    let [finding] = findings.as_slice() else {
        anyhow::bail!("expected one finding, got {findings:?}");
    };
    assert_eq!(finding.subject, "steerer:verifier");
    assert_eq!(finding.figure_value, 36.0);
    Ok(())
}

#[test]
fn daily_job_is_late_after_thirty_six_hours_but_a_weekly_job_is_not() -> anyhow::Result<()> {
    let now = at("2026-11-14T12:00:00Z")?;
    let ran = |name: &str, success: Option<&str>| -> anyhow::Result<JobRow> {
        Ok(JobRow {
            job_name: name.to_owned(),
            last_success: success.map(at).transpose()?,
            last_run: at("2026-11-13T12:00:00Z")?,
            last_result: Some("exited".to_owned()),
        })
    };
    let rows = [
        ran("compressor_loop", Some("2026-11-12T12:00:00Z"))?,
        ran("learning_audit", Some("2026-11-10T12:00:00Z"))?,
        ran("lessons_commit", Some("2026-11-14T01:30:00Z"))?,
        ran("usage_audit", None)?,
    ];
    let findings = job_findings(&rows, now);
    let subjects: Vec<&str> = findings.iter().map(|f| f.subject.as_str()).collect();
    assert_eq!(subjects, vec!["job:compressor_loop", "job:usage_audit"]);
    assert!(findings[1].figure_text.contains("never succeeded"));
    Ok(())
}

#[test]
fn unaudited_closed_fleets_make_one_finding_that_counts_them() -> anyhow::Result<()> {
    let rows = [
        UnauditedFleet {
            slug: "open-issues".to_owned(),
            finished_at: at("2026-11-12T07:22:39Z")?,
        },
        UnauditedFleet {
            slug: "release-alpha-2".to_owned(),
            finished_at: at("2026-11-11T11:39:22Z")?,
        },
    ];
    let findings = unaudited_fleet_findings(&rows);
    let [finding] = findings.as_slice() else {
        anyhow::bail!("expected one finding, got {findings:?}");
    };
    assert_eq!(finding.figure_value, 2.0);
    assert!(finding.figure_text.contains("open-issues, release-alpha-2"));
    assert!(unaudited_fleet_findings(&[]).is_empty());
    Ok(())
}

#[test]
fn coverage_below_the_floor_is_a_finding_and_too_few_items_is_no_verdict() -> anyhow::Result<()> {
    let low = CoverageRow {
        total: 783,
        covered: 234,
    };
    let findings = coverage_findings(
        "supply_verdict_coverage",
        "supplied_items",
        "items",
        low,
        20,
    )
    .ok_or_else(|| anyhow::anyhow!("no verdict"))?;
    let [finding] = findings.as_slice() else {
        anyhow::bail!("expected one finding, got {findings:?}");
    };
    assert!((finding.figure_value - 29.885).abs() < 0.01);

    let healthy = CoverageRow {
        total: 100,
        covered: 90,
    };
    assert_eq!(
        coverage_findings(
            "supply_verdict_coverage",
            "supplied_items",
            "items",
            healthy,
            20
        ),
        Some(Vec::new())
    );
    let few = CoverageRow {
        total: 5,
        covered: 0,
    };
    assert_eq!(
        coverage_findings(
            "supply_verdict_coverage",
            "supplied_items",
            "items",
            few,
            20
        ),
        None
    );
    Ok(())
}

#[test]
fn lessons_never_retired_is_a_finding_once_there_are_enough_of_them() {
    let none_retired = RetirementRow {
        lessons: 2452,
        retired: 0,
    };
    assert_eq!(retirement_findings(none_retired).map(|f| f.len()), Some(1));
    let some_retired = RetirementRow {
        lessons: 2452,
        retired: 3,
    };
    assert_eq!(retirement_findings(some_retired), Some(Vec::new()));
    let few = RetirementRow {
        lessons: 10,
        retired: 0,
    };
    assert_eq!(retirement_findings(few), None);
}

#[test]
fn a_failed_check_is_reported_and_not_counted_as_checked() {
    let mut outcome = PassOutcome::default();
    outcome.absorb(&["stage_applied"], Err(sqlx::Error::PoolTimedOut));
    outcome.absorb(&["toggle_off"], Ok(Some(Vec::<Finding>::new())));
    outcome.absorb(&["fleet_event_written"], Ok(None));
    assert_eq!(outcome.checked, vec!["toggle_off"]);
    assert_eq!(outcome.failed.len(), 1);
    assert_eq!(outcome.failed[0].0, "stage_applied");
}
