use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;

pub const CHECKS_VERSION: &str = "checks-v1";

const REJECTED_PREFIX: &str = "The user doesn't want to proceed with this tool use";

const KIND_PATTERNS: [(&str, &str); 7] = [
    (
        "test",
        r"\bcargo\s+(test|nextest)\b|\bscripts/cx\s+(test|nextest)\b|\bmake\s+test\S*|\bscripts/(test-suite|sb-test)\b|\bpytest\b|\bgo\s+test\b|\bvitest\b|\bpnpm\s+(test|exec\s+vitest)\b|\bnpm\s+(test|run\s+test)\b",
    ),
    (
        "lint",
        r"\bcargo\s+clippy\b|\bscripts/cx\s+clippy\b|\bmake\s+lint\b|\bscripts/(lint-rust\S*|fidelity-check|rule-checks/\S+)|\bpnpm\s+(lint|exec\s+eslint)\b|\bcargo\s+fmt\s+--check\b",
    ),
    (
        "typecheck",
        r"\bsvelte-check\b|\btsc\b|\bpnpm\s+(check|exec\s+svelte-check)\b",
    ),
    (
        "build",
        r"\bcargo\s+(build|check)\b|\bscripts/cx\s+(build|check)\b|\bmake\s+build\b|\bpnpm\s+build\b|\bgo\s+build\b",
    ),
    ("perf", r"\bscripts/db-perf-check\b"),
    ("migrate", r"\bmike\s+migrate-check\b"),
    ("gate", r"\bscripts/gate\b"),
];

static KINDS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    KIND_PATTERNS
        .iter()
        .map(|(kind, pattern)| match Regex::new(pattern) {
            Ok(re) => (*kind, re),
            Err(e) => panic!("invalid check pattern for {kind}: {e}"),
        })
        .collect()
});

static EXIT_LINE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"(?m)^Exit code (\d+)") {
    Ok(re) => re,
    Err(e) => panic!("invalid exit code pattern: {e}"),
});

#[derive(Debug, Clone)]
pub struct BashResult {
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct RegistryRow {
    pub work_item_id: i64,
    pub state: String,
    pub exit_code: Option<i32>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct BashUse {
    pub tool_use_id: String,
    pub request_id: i64,
    pub at: DateTime<Utc>,
    pub command: String,
    pub result: Option<BashResult>,
    pub registry: Option<RegistryRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub tool_use_id: String,
    pub request_id: i64,
    pub at: DateTime<Utc>,
    pub kind: &'static str,
    pub command: String,
    pub exit_code: Option<i32>,
    pub rejected: bool,
    pub background: bool,
    pub work_item_id: Option<i64>,
    pub pending: bool,
}

pub fn check_kind(command: &str) -> Option<&'static str> {
    KINDS
        .iter()
        .find(|(_, pattern)| pattern.is_match(command))
        .map(|(kind, _)| *kind)
}

pub fn parse_exit_code(result: &BashResult) -> Option<i32> {
    if let Some(captures) = EXIT_LINE.captures(&result.content) {
        return captures.get(1).and_then(|m| m.as_str().parse().ok());
    }
    (!result.is_error).then_some(0)
}

fn is_final(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "killed")
}

fn registry_exit(row: &RegistryRow, closed_at: Option<DateTime<Utc>>) -> Option<Option<i32>> {
    let finished_before_close = match (row.finished_at, closed_at) {
        (Some(finished), Some(closed)) => finished <= closed,
        (Some(_), None) => true,
        (None, _) => false,
    };
    (is_final(&row.state) && finished_before_close).then_some(row.exit_code)
}

fn build_check(kind: &'static str, usage: &BashUse, closed_at: Option<DateTime<Utc>>) -> Check {
    let rejected = usage
        .result
        .as_ref()
        .is_some_and(|result| result.content.starts_with(REJECTED_PREFIX));
    let result_exit = usage
        .result
        .as_ref()
        .filter(|_| !rejected)
        .and_then(parse_exit_code);
    let (exit_code, pending) = match &usage.registry {
        Some(row) => match registry_exit(row, closed_at) {
            Some(exit) => (exit, false),
            None => (None, true),
        },
        None => (result_exit, usage.result.is_none()),
    };
    Check {
        tool_use_id: usage.tool_use_id.clone(),
        request_id: usage.request_id,
        at: usage.at,
        kind,
        command: usage.command.clone(),
        exit_code,
        rejected,
        background: usage.registry.is_some(),
        work_item_id: usage.registry.as_ref().map(|row| row.work_item_id),
        pending,
    }
}

pub fn derive_checks(uses: &[BashUse], closed_at: Option<DateTime<Utc>>) -> Vec<Check> {
    uses.iter()
        .filter_map(|usage| {
            check_kind(&usage.command).map(|kind| build_check(kind, usage, closed_at))
        })
        .collect()
}

pub fn failed_count(checks: &[Check]) -> usize {
    checks
        .iter()
        .filter(|check| check.exit_code.is_some_and(|code| code > 0))
        .count()
}
