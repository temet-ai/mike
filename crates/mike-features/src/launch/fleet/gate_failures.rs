use std::borrow::Cow;
use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

#[path = "gate_failure_routes.rs"]
pub mod routes;
pub use routes::tell_owners;

fn ansi() -> Option<&'static Regex> {
    static ANSI: OnceLock<Option<Regex>> = OnceLock::new();
    ANSI.get_or_init(|| Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").ok())
        .as_ref()
}

fn runner_prefix() -> Option<&'static Regex> {
    static PREFIX: OnceLock<Option<Regex>> = OnceLock::new();
    PREFIX
        .get_or_init(|| Regex::new(r"^(?:\[[^\]]*\]\s*|\(\d+/\d+\)\s*)+").ok())
        .as_ref()
}

pub fn inventory(output: &str) -> Vec<String> {
    let clean = ansi().map_or(Cow::Borrowed(output), |pattern| {
        pattern.replace_all(output, "")
    });
    let mut found = BTreeSet::new();
    let mut in_failure_list = false;
    for raw in clean.lines() {
        let line = raw.trim();
        if line == "failures:" {
            in_failure_list = true;
            continue;
        }
        if let Some(failure) = marked(line) {
            found.insert(failure);
        } else if in_failure_list && raw.starts_with("    ") && test_name(line) {
            found.insert(format!("FAIL {line}"));
        } else if !line.is_empty() {
            in_failure_list = false;
        }
    }
    json_failures(&clean, &mut found);
    found.into_iter().collect()
}

fn test_name(line: &str) -> bool {
    !line.is_empty()
        && line
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':'))
}

fn marked(line: &str) -> Option<String> {
    if let Some(test) = line
        .strip_prefix("test ")
        .and_then(|s| s.strip_suffix(" ... FAILED"))
    {
        return Some(format!("FAIL {}", test.trim()));
    }
    for marker in ["FAIL", "TIMEOUT", "LEAK", "FAILED", "ERROR", "❯", "×", "✕"] {
        let Some(rest) = line
            .strip_prefix(marker)
            .filter(|s| s.starts_with(char::is_whitespace))
        else {
            continue;
        };
        let name = runner_prefix().map_or(Cow::Borrowed(rest.trim()), |pattern| {
            pattern.replace(rest.trim(), "")
        });
        let name = if matches!(marker, "FAILED" | "ERROR") {
            name.split(" - ").next().unwrap_or_default()
        } else {
            name.as_ref()
        };
        if !name.is_empty() {
            let status = if marker == "TIMEOUT" {
                "TIMEOUT"
            } else {
                "FAIL"
            };
            return Some(format!("{status} {name}"));
        }
    }
    None
}

fn json_failures(output: &str, found: &mut BTreeSet<String>) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
        json_results(&value, found);
        return;
    }
    for line in output
        .lines()
        .filter(|line| line.trim_start().starts_with('{'))
    {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            json_results(&value, found);
        }
    }
}

fn json_results(value: &serde_json::Value, found: &mut BTreeSet<String>) {
    let Some(results) = value.get("testResults").and_then(|v| v.as_array()) else {
        return;
    };
    for result in results {
        let file = result
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown file");
        let Some(assertions) = result.get("assertionResults").and_then(|v| v.as_array()) else {
            continue;
        };
        for assertion in assertions.iter().filter(|a| a["status"] == "failed") {
            if let Some(name) = assertion.get("fullName").and_then(|v| v.as_str()) {
                found.insert(format!("FAIL {file} > {name}"));
            }
        }
    }
}

pub fn summary(failures: &[String], log: &str) -> String {
    let heading = if failures.is_empty() {
        "Failing-test inventory unparsed: no recognized test IDs in captured output.".to_owned()
    } else {
        format!(
            "Recognized failing tests/timeouts ({} unique; unparsed or unrun tests may be missing):\n{}",
            failures.len(),
            failures
                .iter()
                .map(|name| format!("  {name}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    format!("{heading}\nFull captured output: {log}")
}

#[cfg(test)]
#[path = "gate_failures_tests.rs"]
pub mod tests;

#[cfg(feature = "test-seams")]
#[path = "gate_failure_test_seams.rs"]
pub mod test_seams;
