use std::time::Duration;

use chrono::Weekday;

use super::schedule::{Cadence, Period};
use crate::settings::meta_memory_loop::MEMORY_REVIEW_ENABLED_KEY;
use crate::settings::meta_scheduled_jobs::{
    COMPRESSOR_LOOP_ENABLED_KEY, LEARNING_AUDIT_ENABLED_KEY, LEARNING_PROMOTE_ENABLED_KEY,
    LESSON_RELATIONS_ENABLED_KEY, LESSONS_COMMIT_ENABLED_KEY, USAGE_AUDIT_ENABLED_KEY,
};

pub(crate) struct Job {
    pub(crate) name: &'static str,
    pub(crate) label: &'static str,
    pub(crate) description: &'static str,
    pub(crate) script: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) timeout: Duration,
    pub(super) cadence: Cadence,
    pub(super) nice: u8,
    pub(super) leased: bool,
    pub(super) alarm_exit: Option<i32>,
    pub(crate) enabled_key: &'static str,
    pub(crate) schedule_text: &'static str,
    pub(crate) failure_hint: &'static str,
}

pub(super) const PROMOTE: Job = Job {
    name: "learning_promote",
    label: "Learning promotion",
    description: "Turns accepted lessons into project memories and skills in the project folders on this machine, so Claude Code sessions here find them.",
    script: "scripts/learning-promote",
    args: &[],
    timeout: Duration::from_secs(15 * 60),
    cadence: Cadence {
        weekday: None,
        hours: &[20, 21, 22, 23, 0, 1, 2, 3],
        minute: 30,
        period: Period::Hour,
    },
    nice: 10,
    leased: false,
    alarm_exit: Some(1),
    enabled_key: LEARNING_PROMOTE_ENABLED_KEY,
    schedule_text: "hourly at :30 from 20:30 to 03:30 UTC",
    failure_hint: "Exit code 1 means generated files conflict with manual edits.",
};

pub(super) const AUDIT: Job = Job {
    name: "learning_audit",
    label: "Weekly learning audit",
    description: "Reviews the previous week's sessions, lessons and tool use and publishes a report, so you can see whether the learning is helping.",
    script: "scripts/learning-audit",
    args: &["--weekly", "--sync"],
    timeout: Duration::from_secs(30 * 60),
    cadence: Cadence {
        weekday: Some(Weekday::Mon),
        hours: &[1, 2, 3, 4, 5, 6],
        minute: 0,
        period: Period::Week,
    },
    nice: 10,
    leased: true,
    alarm_exit: None,
    enabled_key: LEARNING_AUDIT_ENABLED_KEY,
    schedule_text: "weekly, once on Monday between 01:00 and 06:59 UTC",
    failure_hint: "The audit reads the database with psql and the inspect API on 127.0.0.1:9989, so check that both answer.",
};

pub(super) const COMPRESSOR_LOOP: Job = Job {
    name: "compressor_loop",
    label: "File-read compressor loop",
    description: "Compares sessions that got shortened file reads with those that did not and adjusts the compressor's settings, so it keeps only the cuts that do no harm.",
    script: "scripts/compressor-loop",
    args: &["--days", "7", "--apply"],
    timeout: Duration::from_secs(20 * 60),
    cadence: Cadence {
        weekday: None,
        hours: &[4, 5, 6],
        minute: 30,
        period: Period::Day,
    },
    nice: 10,
    leased: true,
    alarm_exit: None,
    enabled_key: COMPRESSOR_LOOP_ENABLED_KEY,
    schedule_text: "daily, once between 04:30 and 06:59 UTC",
    failure_hint: "The loop reads the database with psql (or docker exec) and changes settings through the inspect API on 127.0.0.1:9989, so check that both answer.",
};

#[cfg(test)]
pub(super) const SIX_HOURLY_FIXTURE: Job = Job {
    name: "six_hourly_fixture",
    label: "Six-hourly fixture",
    description: "A test job on the hour every six hours.",
    script: "scripts/six-hourly-fixture",
    args: &[],
    timeout: Duration::from_secs(30 * 60),
    cadence: Cadence {
        weekday: None,
        hours: &[0, 6, 12, 18],
        minute: 0,
        period: Period::Hour,
    },
    nice: 19,
    leased: false,
    alarm_exit: None,
    enabled_key: "six_hourly_fixture.enabled",
    schedule_text: "every six hours, at 00:00, 06:00, 12:00 and 18:00 UTC",
    failure_hint: "A fixture.",
};

pub(super) const LESSONS_COMMIT: Job = Job {
    name: "lessons_commit",
    label: "Lesson file commit",
    description: "Commits the lesson files the knowledge collector writes into .claude/lessons of this checkout, so the durable copy of the lessons is in git.",
    script: "scripts/lessons-commit",
    args: &[],
    timeout: Duration::from_secs(10 * 60),
    cadence: Cadence {
        weekday: None,
        hours: &[1, 2, 3],
        minute: 30,
        period: Period::Day,
    },
    nice: 10,
    leased: false,
    alarm_exit: None,
    enabled_key: LESSONS_COMMIT_ENABLED_KEY,
    schedule_text: "daily, once between 01:30 and 03:59 UTC (03:30 in Johannesburg)",
    failure_hint: "It commits only .claude/lessons on main, and stands aside while another git operation is in progress, so check that git status works in the checkout.",
};

pub(super) const USAGE_AUDIT: Job = Job {
    name: "usage_audit",
    label: "Weekly usage audit",
    description: "Reports where the week's Claude tokens went and raises an alarm when the cache re-write rate, context size, dead steering rules or evaluation metrics look wrong.",
    script: "scripts/usage-audit.sh",
    args: &[],
    timeout: Duration::from_secs(30 * 60),
    cadence: Cadence {
        weekday: Some(Weekday::Sun),
        hours: &[6, 7, 8, 9, 10],
        minute: 0,
        period: Period::Week,
    },
    nice: 10,
    leased: true,
    alarm_exit: Some(1),
    enabled_key: USAGE_AUDIT_ENABLED_KEY,
    schedule_text: "weekly, once on Sunday between 06:00 and 10:59 UTC (08:00 in Johannesburg)",
    failure_hint: "The audit reads the database with psql, so check that MIKE_DATABASE_URL in the checkout's .env answers. Exit code 1 is an alarm, not a failure.",
};

pub(super) const LESSON_RELATIONS: Job = Job {
    name: "lesson_relations",
    label: "Lesson relation detector",
    description: "Finds lessons that supersede, refine, repeat or contradict each other and has a second model adjudicate each claim, so the nightly consolidation can retire a lesson a newer one contradicts.",
    script: "scripts/lesson-relations",
    args: &[],
    timeout: Duration::from_secs(45 * 60),
    cadence: Cadence {
        weekday: None,
        hours: &[22, 23],
        minute: 0,
        period: Period::Day,
    },
    nice: 10,
    leased: true,
    alarm_exit: None,
    enabled_key: LESSON_RELATIONS_ENABLED_KEY,
    schedule_text: "daily, once between 22:00 and 23:59 UTC, before the nightly lesson consolidation",
    failure_hint: "It runs `mike detect-lesson-relations` (MIKE_BIN, else mike on PATH, else ~/.local/bin/mike) and calls the proposer and adjudicator models, so check that both models answer and that lesson_relations.enabled is on.",
};

pub(super) const MEMORY_REVIEW: Job = Job {
    name: "memory_review",
    label: "Daily memory review",
    description: "A Claude Code session reads what memory returned to sessions since the last review, judges whether each return helped, and records the fix on the memory-keeper thread and the issue that covers it.",
    script: "scripts/memory-review",
    args: &[],
    timeout: Duration::from_secs(45 * 60),
    cadence: Cadence {
        weekday: None,
        hours: &[5, 6, 7],
        minute: 15,
        period: Period::Day,
    },
    nice: 10,
    leased: true,
    alarm_exit: None,
    enabled_key: MEMORY_REVIEW_ENABLED_KEY,
    schedule_text: "daily, once between 05:15 and 07:59 UTC, after the overnight supply verdicts",
    failure_hint: "It runs `claude -p` (CLAUDE_BIN, else claude on PATH, else ~/.local/bin/claude) through the proxy on localhost:9988, so check that claude is logged in on this machine and that the proxy answers.",
};

pub(crate) static JOBS: [Job; 7] = [
    PROMOTE,
    AUDIT,
    COMPRESSOR_LOOP,
    LESSONS_COMMIT,
    USAGE_AUDIT,
    LESSON_RELATIONS,
    MEMORY_REVIEW,
];
