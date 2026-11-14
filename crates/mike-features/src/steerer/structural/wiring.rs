use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;

use super::never_fired::NeverFiredFinding;
use super::wiring_checks;

pub const STAGE_APPLIED: &str = "stage_applied";
pub const FLEET_EVENT_WRITTEN: &str = "fleet_event_written";
pub const TOGGLE_OFF: &str = "toggle_off";
pub const SHADOW_NOT_PROMOTED: &str = "shadow_not_promoted";
pub const STEERER_DORMANT: &str = "steerer_dormant";
pub const JOB_NOT_SUCCEEDING: &str = "job_not_succeeding";
pub const CLOSED_FLEET_UNAUDITED: &str = "closed_fleet_unaudited";
pub const SUPPLY_VERDICT_COVERAGE: &str = "supply_verdict_coverage";
pub const SEARCH_GRADE_COVERAGE: &str = "search_grade_coverage";
pub const LESSON_RETIREMENT: &str = "lesson_retirement";

pub const STAGE_SAMPLE_REQUESTS: i64 = 2000;
pub const STAGE_MIN_SEEN: i64 = 200;
pub const FLEET_EVENT_WINDOW_DAYS: i32 = 7;
pub const FLEET_EVENT_MIN_ACTIVITY: i64 = 50;
pub const TOGGLE_OFF_DAYS: i64 = 14;
pub const SHADOW_DAYS: i64 = 14;
pub const DORMANT_DAYS: i64 = 14;
pub const JOB_DAILY_HOURS: i64 = 36;
pub const JOB_WEEKLY_HOURS: i64 = 192;
pub const UNAUDITED_FLEET_DAYS: i32 = 14;
pub const COVERAGE_FLOOR_PERCENT: f64 = 80.0;
pub const VERDICT_MIN_ITEMS: i64 = 20;
pub const GRADE_MIN_EVENTS: i64 = 10;
pub const RETIREMENT_MIN_LESSONS: i64 = 100;
const WEEKLY_JOBS: &[&str] = &["learning_audit", "usage_audit"];
const UNAUDITED_FLEETS_NAMED: usize = 8;

static LAST_PASS_SECS: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub check_kind: &'static str,
    pub subject: String,
    pub figure_value: f64,
    pub figure_text: String,
}

impl Finding {
    fn new(check_kind: &'static str, subject: String, figure_value: f64, text: String) -> Self {
        Self {
            check_kind,
            subject,
            figure_value,
            figure_text: text,
        }
    }
}

#[derive(Debug, Default)]
pub struct PassOutcome {
    pub checked: Vec<&'static str>,
    pub findings: Vec<Finding>,
    pub failed: Vec<(&'static str, String)>,
}

impl PassOutcome {
    pub fn absorb(
        &mut self,
        kinds: &[&'static str],
        result: Result<Option<Vec<Finding>>, sqlx::Error>,
    ) {
        match result {
            Ok(Some(findings)) => {
                self.checked.extend_from_slice(kinds);
                self.findings.extend(findings);
            }
            Ok(None) => {}
            Err(error) => self
                .failed
                .extend(kinds.iter().map(|kind| (*kind, error.to_string()))),
        }
    }
}

pub fn mark_pass(at: DateTime<Utc>) {
    LAST_PASS_SECS.store(at.timestamp(), Ordering::Relaxed);
}

pub fn last_pass() -> Option<DateTime<Utc>> {
    match LAST_PASS_SECS.load(Ordering::Relaxed) {
        0 => None,
        secs => DateTime::from_timestamp(secs, 0),
    }
}

pub async fn run_checks(pool: &PgPool, now: DateTime<Utc>) -> PassOutcome {
    let mut outcome = PassOutcome::default();
    outcome.absorb(&[STAGE_APPLIED], wiring_checks::stage_applied(pool).await);
    outcome.absorb(
        &[FLEET_EVENT_WRITTEN],
        wiring_checks::fleet_events_written(pool).await,
    );
    outcome.absorb(&[TOGGLE_OFF], wiring_checks::toggles_off(pool, now).await);
    outcome.absorb(
        &[SHADOW_NOT_PROMOTED, STEERER_DORMANT],
        wiring_checks::steerer_state(pool, now).await,
    );
    outcome.absorb(
        &[JOB_NOT_SUCCEEDING],
        wiring_checks::jobs_not_succeeding(pool, now).await,
    );
    outcome.absorb(
        &[CLOSED_FLEET_UNAUDITED],
        wiring_checks::closed_fleets_unaudited(pool).await,
    );
    outcome.absorb(
        &[SUPPLY_VERDICT_COVERAGE],
        wiring_checks::supply_verdict_coverage(pool, now).await,
    );
    outcome.absorb(
        &[SEARCH_GRADE_COVERAGE],
        wiring_checks::search_grade_coverage(pool, now).await,
    );
    outcome.absorb(
        &[LESSON_RETIREMENT],
        wiring_checks::lesson_retirement(pool).await,
    );
    outcome
}

#[derive(Debug, Clone, PartialEq)]
pub struct StageRow {
    pub stage: String,
    pub seen: i64,
    pub applied: i64,
    pub top_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToggleRow {
    pub key: String,
    pub operator_set: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteererActivityRow {
    pub name: String,
    pub shadow: i64,
    pub live: i64,
    pub first_shadow: Option<DateTime<Utc>>,
    pub last_evaluation: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowQuestionRow {
    pub steerer: String,
    pub question: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobRow {
    pub job_name: String,
    pub last_success: Option<DateTime<Utc>>,
    pub last_run: DateTime<Utc>,
    pub last_result: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnauditedFleet {
    pub slug: String,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverageRow {
    pub total: i64,
    pub covered: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetirementRow {
    pub lessons: i64,
    pub retired: i64,
}

fn percent(part: i64, whole: i64) -> f64 {
    if whole <= 0 {
        return 0.0;
    }
    part as f64 * 100.0 / whole as f64
}

fn whole_days(now: DateTime<Utc>, since: DateTime<Utc>) -> i64 {
    (now - since).num_days()
}

pub fn stage_findings(rows: &[StageRow]) -> Vec<Finding> {
    rows.iter()
        .filter(|row| stage_is_idle(row))
        .map(stage_finding)
        .collect()
}

fn stage_is_idle(row: &StageRow) -> bool {
    let served_by_client = row
        .top_reason
        .as_deref()
        .is_some_and(|reason| reason.ends_with("_served_by_client"));
    row.seen >= STAGE_MIN_SEEN && row.applied * 1000 < row.seen && !served_by_client
}

fn stage_finding(row: &StageRow) -> Finding {
    let rate = percent(row.applied, row.seen);
    let reason = row.top_reason.as_deref().unwrap_or("none recorded");
    Finding::new(
        STAGE_APPLIED,
        format!("stage:{}", row.stage),
        rate,
        format!(
            "{} applied on {} of {} requests in the last 24 hours ({rate:.2}%); most common reason: {reason}",
            row.stage, row.applied, row.seen
        ),
    )
}

pub fn fleet_event_findings(finding: &NeverFiredFinding) -> Option<Vec<Finding>> {
    if finding.total_observations < FLEET_EVENT_MIN_ACTIVITY {
        return None;
    }
    let findings = finding
        .never_observed
        .iter()
        .map(|kind| {
            Finding::new(
                FLEET_EVENT_WRITTEN,
                format!("event:{kind}"),
                0.0,
                format!(
                    "fleet event {kind} has a writer and 0 rows in the last {FLEET_EVENT_WINDOW_DAYS} days, while {} other fleet events were written",
                    finding.total_observations
                ),
            )
        })
        .collect();
    Some(findings)
}

pub fn toggle_findings(rows: &[ToggleRow], now: DateTime<Utc>) -> Vec<Finding> {
    rows.iter()
        .filter_map(|row| {
            let days = whole_days(now, row.updated_at);
            let who = if row.operator_set {
                "set off by the operator"
            } else {
                "off by default"
            };
            (days > TOGGLE_OFF_DAYS).then(|| {
                Finding::new(
                    TOGGLE_OFF,
                    format!("toggle:{}", row.key),
                    days as f64,
                    format!("toggle {} has been off for {days} days ({who})", row.key),
                )
            })
        })
        .collect()
}

pub fn shadow_findings(
    questions: &[ShadowQuestionRow],
    steerers: &[SteererActivityRow],
    now: DateTime<Utc>,
) -> Vec<Finding> {
    let from_questions = questions.iter().filter_map(|row| {
        let days = whole_days(now, row.updated_at);
        (days >= SHADOW_DAYS).then(|| {
            Finding::new(
                SHADOW_NOT_PROMOTED,
                format!("question:{}/{}", row.steerer, row.question),
                days as f64,
                format!(
                    "{} question {} has been in shadow mode for {days} days without promotion",
                    row.steerer, row.question
                ),
            )
        })
    });
    let from_evaluations = steerers.iter().filter_map(|row| {
        let first = row.first_shadow?;
        let days = whole_days(now, first);
        (row.live == 0 && row.shadow > 0 && days >= SHADOW_DAYS).then(|| {
            Finding::new(
                SHADOW_NOT_PROMOTED,
                format!("steerer:{}", row.name),
                days as f64,
                format!(
                    "{} has {} shadow evaluations since {} and no live evaluation",
                    row.name,
                    row.shadow,
                    first.format("%Y-%m-%d")
                ),
            )
        })
    });
    from_questions.chain(from_evaluations).collect()
}

pub fn dormant_findings(steerers: &[SteererActivityRow], now: DateTime<Utc>) -> Vec<Finding> {
    steerers
        .iter()
        .filter_map(|row| {
            let last = row.last_evaluation?;
            let days = whole_days(now, last);
            (days >= DORMANT_DAYS).then(|| {
                Finding::new(
                    STEERER_DORMANT,
                    format!("steerer:{}", row.name),
                    days as f64,
                    format!(
                        "steerer {} has had no evaluation for {days} days (last {})",
                        row.name,
                        last.format("%Y-%m-%d")
                    ),
                )
            })
        })
        .collect()
}

fn job_allowance(job_name: &str) -> Duration {
    if WEEKLY_JOBS.contains(&job_name) {
        Duration::hours(JOB_WEEKLY_HOURS)
    } else {
        Duration::hours(JOB_DAILY_HOURS)
    }
}

pub fn job_findings(rows: &[JobRow], now: DateTime<Utc>) -> Vec<Finding> {
    rows.iter()
        .filter(|row| {
            row.last_success
                .is_none_or(|success| now - success > job_allowance(&row.job_name))
        })
        .map(|row| job_finding(row, now))
        .collect()
}

fn job_finding(row: &JobRow, now: DateTime<Utc>) -> Finding {
    let result = row.last_result.as_deref().unwrap_or("unknown");
    let (since, text) = match row.last_success {
        Some(success) => (
            success,
            format!(
                "job {} last succeeded {} hours ago; its latest run ended {result}",
                row.job_name,
                (now - success).num_hours()
            ),
        ),
        None => (
            row.last_run,
            format!(
                "job {} has never succeeded; its latest run ended {result} {} hours ago",
                row.job_name,
                (now - row.last_run).num_hours()
            ),
        ),
    };
    Finding::new(
        JOB_NOT_SUCCEEDING,
        format!("job:{}", row.job_name),
        (now - since).num_hours() as f64,
        text,
    )
}

pub fn unaudited_fleet_findings(rows: &[UnauditedFleet]) -> Vec<Finding> {
    let Some(newest) = rows.first() else {
        return Vec::new();
    };
    let named: Vec<&str> = rows
        .iter()
        .take(UNAUDITED_FLEETS_NAMED)
        .map(|row| row.slug.as_str())
        .collect();
    vec![Finding::new(
        CLOSED_FLEET_UNAUDITED,
        "closed_fleets".to_owned(),
        rows.len() as f64,
        format!(
            "{} fleets closed in the last {UNAUDITED_FLEET_DAYS} days have no rule-audit run (newest closed {}): {}",
            rows.len(),
            newest.finished_at.format("%Y-%m-%d %H:%M"),
            named.join(", ")
        ),
    )]
}

pub fn coverage_findings(
    kind: &'static str,
    subject: &str,
    noun: &str,
    row: CoverageRow,
    min_total: i64,
) -> Option<Vec<Finding>> {
    if row.total < min_total {
        return None;
    }
    let rate = percent(row.covered, row.total);
    if rate >= COVERAGE_FLOOR_PERCENT {
        return Some(Vec::new());
    }
    Some(vec![Finding::new(
        kind,
        subject.to_owned(),
        rate,
        format!(
            "{} of {} {noun} ({rate:.1}%, floor {COVERAGE_FLOOR_PERCENT:.0}%)",
            row.covered, row.total
        ),
    )])
}

pub fn retirement_findings(row: RetirementRow) -> Option<Vec<Finding>> {
    if row.lessons < RETIREMENT_MIN_LESSONS {
        return None;
    }
    if row.retired > 0 {
        return Some(Vec::new());
    }
    Some(vec![Finding::new(
        LESSON_RETIREMENT,
        "lessons".to_owned(),
        0.0,
        format!(
            "0 of {} lessons have ever been retired by outcome",
            row.lessons
        ),
    )])
}
