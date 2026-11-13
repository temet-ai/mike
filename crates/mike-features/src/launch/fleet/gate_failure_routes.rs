use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use uuid::Uuid;

use super::inventory;
use crate::launch::fleet::checks::normal;
use crate::launch::fleet::commit_paths::covers;
use crate::launch::fleet::definition::{FleetDefinition, TaskDefinition};
use crate::launch::fleet::gate::Target;
use crate::launch::fleet::watch::git;
use crate::store::persistence::PersistHandle;

#[path = "gate_failure_delivery.rs"]
pub mod delivery;
pub use delivery::tell_owners;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Routes {
    owners: BTreeMap<Uuid, BTreeSet<String>>,
    unresolved: BTreeSet<String>,
    tasks: BTreeMap<String, Option<Uuid>>,
}

async fn tracked_files(def: &FleetDefinition, target: &Target) -> Option<Vec<String>> {
    let bytes = git(
        &def.repo,
        &["ls-tree", "-r", "--name-only", "-z", &target.commit],
        None,
    )
    .await?;
    Some(
        bytes
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .filter_map(|name| String::from_utf8(name.to_vec()).ok())
            .collect(),
    )
}

fn routes(def: &FleetDefinition, target: &Target, files: &[String], output: &str) -> Routes {
    let mut routed = Routes::default();
    let failures = inventory(output);
    if failures.is_empty() {
        routed.unresolved.insert(
            "Failing-test inventory unparsed: no recognized test IDs in captured output.".into(),
        );
    }
    for failure in failures {
        let paths = failure_paths(def, target, files, &failure);
        if paths.is_empty() {
            routed.unresolved.insert(format!(
                "{failure}: no committed file could be identified; ownership unknown"
            ));
            continue;
        }
        let ambiguous = paths.len() > 1;
        for path in &paths {
            route_path(def, &mut routed, path, &failure, ambiguous);
        }
    }
    routed
}

fn reported_file(failure: &str) -> Option<&str> {
    let (_, failure) = failure.split_once(' ')?;
    failure
        .split(" > ")
        .next()?
        .split("::")
        .next()?
        .split(" (")
        .next()?
        .split(':')
        .next()
        .map(str::trim)
        .filter(|file| !file.is_empty())
}

fn relative_file(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.components() {
        match part {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn failure_paths(
    def: &FleetDefinition,
    target: &Target,
    files: &[String],
    failure: &str,
) -> Vec<String> {
    let Some(reported) = reported_file(failure) else {
        return Vec::new();
    };
    let path = Path::new(reported);
    let relative = if path.is_absolute() {
        path.strip_prefix(&target.slot)
            .or_else(|_| path.strip_prefix(&def.repo))
            .ok()
            .and_then(relative_file)
    } else {
        relative_file(path)
    };
    let Some(relative) = relative else {
        return Vec::new();
    };
    let suffix = format!("/{relative}");
    files
        .iter()
        .filter(|file| **file == relative || (!path.is_absolute() && file.ends_with(&suffix)))
        .cloned()
        .collect()
}

fn task_owns(task: &TaskDefinition, path: &str) -> bool {
    let owned = task
        .files
        .iter()
        .chain(&task.new_files)
        .map(|file| normal(file))
        .collect::<Vec<_>>();
    covers(&owned, path)
}

fn owners<'a>(def: &'a FleetDefinition, path: &str) -> Vec<(&'a String, &'a TaskDefinition)> {
    if let Some(producer) = crate::launch::fleet::checks_paths::producer_for(def, path)
        && let Some(owner) = def.tasks.get_key_value(producer)
    {
        return vec![owner];
    }
    def.tasks
        .iter()
        .filter(|(_, task)| task_owns(task, path))
        .collect()
}

fn route_path(
    def: &FleetDefinition,
    routed: &mut Routes,
    path: &str,
    failure: &str,
    ambiguous: bool,
) {
    let owners = owners(def, path);
    let detail = format!("{failure} [committed file: {path}]");
    if owners.is_empty() {
        routed.unresolved.insert(format!("{detail}; unowned file"));
    }
    let detail = if ambiguous || owners.len() > 1 {
        let detail = format!(
            "{detail}; ambiguous file/ownership match, investigate before assigning responsibility"
        );
        routed.unresolved.insert(detail.clone());
        detail
    } else {
        detail
    };
    for (id, task) in owners {
        let seat = task.seat.as_deref().unwrap_or("no named seat");
        let detail = format!("task {id} ({seat}): {detail}");
        let session = task.session.as_deref().and_then(|s| s.parse().ok());
        routed.tasks.insert(id.clone(), session);
        match session {
            Some(session) => {
                routed.owners.entry(session).or_default().insert(detail);
            }
            None => {
                routed
                    .unresolved
                    .insert(format!("{detail}; no routable session"));
            }
        }
    }
}

#[cfg(test)]
#[path = "gate_failure_routes_tests.rs"]
mod tests;
