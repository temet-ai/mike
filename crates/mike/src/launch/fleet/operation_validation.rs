use std::collections::{BTreeMap, BTreeSet};

use super::{MAX_CAPACITY, Operation};
use crate::launch::fleet::checks::Missing;
use crate::launch::fleet::definition::FleetDefinition;

pub(crate) fn validate(def: &FleetDefinition) -> Vec<Missing> {
    let mut found = Vec::new();
    let mut commands = BTreeSet::new();
    let mut capacities: BTreeMap<&str, u32> = BTreeMap::new();
    for (name, op) in &def.execution.operations {
        let item = format!("execution.operations.{name}");
        for problem in problems(name, op) {
            found.push(Missing::new(&item, problem));
        }
        for claim in &op.resources {
            let agreed = *capacities
                .entry(claim.name().trim())
                .or_insert(claim.capacity());
            if agreed != claim.capacity() {
                found.push(Missing::new(
                    &item,
                    "a resource must declare the same capacity in every operation that names it",
                ));
            }
        }
        if !commands.insert(op.command.trim()) {
            found.push(Missing::new(
                &item,
                "command already has an operation declaration",
            ));
        }
    }
    found
}

fn problems(name: &str, op: &Operation) -> Vec<&'static str> {
    let mut found = Vec::new();
    if name.trim().is_empty() || op.command.trim().is_empty() || op.command.contains('\0') {
        found.push("name and command must be nonempty, and command must not contain NUL");
    }
    if op.purpose.trim().is_empty() {
        found.push("purpose must describe what a successful command establishes");
    }
    if op.resources.iter().any(|r| {
        let name = r.name();
        name.trim().is_empty() || name.len() > 200 || name.chars().any(char::is_control)
    }) {
        found.push(
            "resource names must be nonempty, at most 200 bytes and contain no control characters",
        );
    }
    if op
        .resources
        .iter()
        .any(|r| r.capacity() == 0 || r.capacity() > MAX_CAPACITY)
    {
        found.push("resource capacity must be between 1 and 64");
    }
    if op.timeout_seconds.is_some_and(|n| n == 0 || n > 7200) {
        found.push("timeout_seconds must be between 1 and 7200");
    }
    if op
        .cache_key
        .as_ref()
        .is_some_and(|key| key.trim().is_empty() || op.inputs.is_empty())
    {
        found.push("reuse requires a nonempty cache_key naming external dependencies/toolchain and explicit inputs");
    }
    if op.inputs.iter().any(|p| {
        p.trim().is_empty()
            || std::path::Path::new(p).is_absolute()
            || p.split('/').any(|s| s == "..")
    }) {
        found.push("inputs must be nonempty repository-relative paths without parent traversal");
    }
    if op
        .environment
        .iter()
        .any(|(name, value)| !environment_name(name) || value.contains('\0'))
    {
        found.push("environment names must be shell identifiers outside MIKE_ control variables; values must not contain NUL");
    }
    found
}

fn environment_name(name: &str) -> bool {
    !name.starts_with("MIKE_")
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
