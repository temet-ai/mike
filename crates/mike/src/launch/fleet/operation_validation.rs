use std::collections::{BTreeMap, BTreeSet};

use super::{Execution, MAX_CAPACITY, Operation, ResourceClaim};
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
        agree_capacities(&item, &op.resources, &mut capacities, &mut found);
        if !commands.insert(op.command.trim()) {
            found.push(Missing::new(
                &item,
                "command already has an operation declaration",
            ));
        }
    }
    for (program, timed) in &def.execution.timed_commands {
        let item = format!("execution.timed_commands.{program}");
        if !bare_program(program) {
            found.push(Missing::new(
                &item,
                format!("`{program}` must be a program name without a path or spaces, because mike writes a file of that name on the seats' PATH"),
            ));
        }
        for problem in resource_problems(&timed.resources) {
            found.push(Missing::new(&item, problem));
        }
        agree_capacities(&item, &timed.resources, &mut capacities, &mut found);
    }
    setting_problems(&def.execution, &mut found);
    found
}

fn agree_capacities<'a>(
    item: &str,
    claims: &'a [ResourceClaim],
    capacities: &mut BTreeMap<&'a str, u32>,
    found: &mut Vec<Missing>,
) {
    for claim in claims {
        let agreed = *capacities
            .entry(claim.name().trim())
            .or_insert(claim.capacity());
        if agreed != claim.capacity() {
            found.push(Missing::new(
                item,
                "a resource must declare the same capacity in every operation that names it",
            ));
        }
    }
}

fn bare_program(program: &str) -> bool {
    !program.is_empty()
        && !matches!(program, "." | "..")
        && program
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

fn setting_problems(execution: &Execution, found: &mut Vec<Missing>) {
    for name in &execution.strip_environment {
        if !environment_name(name) {
            found.push(Missing::new(
                "execution.strip_environment",
                format!("`{name}` must be a shell identifier outside MIKE_ control variables"),
            ));
        }
    }
    let empty = [
        (
            "execution.background_commands",
            &execution.background_commands,
        ),
        ("execution.disabled_plugins", &execution.disabled_plugins),
        (
            "execution.build_directory_settings",
            &execution.build_directory_settings,
        ),
    ];
    for (item, values) in empty {
        if values.iter().any(|value| value.trim().is_empty()) {
            found.push(Missing::new(item, "entries must be nonempty"));
        }
    }
    for manifest in &execution.package_manifests {
        if !relative_path(&manifest.file) || manifest.label.trim().is_empty() {
            found.push(Missing::new(
                "execution.package_manifests",
                format!(
                    "`{}` needs a repository-relative file without parent traversal and a label",
                    manifest.file
                ),
            ));
        }
    }
}

pub(crate) fn unread_warnings(def: &FleetDefinition) -> Vec<Missing> {
    def.execution
        .unread
        .keys()
        .map(|key| {
            Missing::new(
                format!("execution.{key}"),
                format!("`{key}` is not an execution setting mike reads, so it has no effect; docs/LAUNCH_DOMAIN.md lists the settings"),
            )
        })
        .collect()
}

fn problems(name: &str, op: &Operation) -> Vec<&'static str> {
    let mut found = Vec::new();
    if name.trim().is_empty() || op.command.trim().is_empty() || op.command.contains('\0') {
        found.push("name and command must be nonempty, and command must not contain NUL");
    }
    if op.purpose.trim().is_empty() {
        found.push("purpose must describe what a successful command establishes");
    }
    found.extend(resource_problems(&op.resources));
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
    if op.inputs.iter().any(|p| !relative_path(p)) {
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

fn resource_problems(resources: &[ResourceClaim]) -> Vec<&'static str> {
    let mut found = Vec::new();
    if resources.iter().any(|r| {
        let name = r.name();
        name.trim().is_empty() || name.len() > 200 || name.chars().any(char::is_control)
    }) {
        found.push(
            "resource names must be nonempty, at most 200 bytes and contain no control characters",
        );
    }
    if resources
        .iter()
        .any(|r| r.capacity() == 0 || r.capacity() > MAX_CAPACITY)
    {
        found.push("resource capacity must be between 1 and 64");
    }
    found
}

fn relative_path(path: &str) -> bool {
    !path.trim().is_empty()
        && !std::path::Path::new(path).is_absolute()
        && !path.split('/').any(|part| part == "..")
}

fn environment_name(name: &str) -> bool {
    !name.starts_with("MIKE_")
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
