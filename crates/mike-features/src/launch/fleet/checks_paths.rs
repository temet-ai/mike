use std::collections::{BTreeMap, BTreeSet};

use super::checks::{Missing, normal};
use super::definition::{FleetDefinition, TaskDefinition};
use super::operation::PackageManifest;

type Overlap = (String, String, String);

fn contains(scope: &str, path: &str) -> bool {
    let scope = normal(scope);
    let path = normal(path);
    !scope.is_empty() && (scope == path || path.starts_with(&format!("{scope}/")))
}

fn valid_path(path: &str) -> bool {
    path == path.trim()
        && !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['*', '?', '[', '\\', '`'])
        && path
            .trim_end_matches('/')
            .split('/')
            .all(|part| !matches!(part, "" | "." | ".."))
}

fn owns(task: &TaskDefinition, path: &str) -> bool {
    task.files
        .iter()
        .chain(&task.new_files)
        .any(|file| contains(file, path))
}

fn shared(def: &FleetDefinition, path: &str) -> bool {
    def.shared_files.iter().any(|file| contains(file, path))
}

fn producer_problem(def: &FleetDefinition, path: &str, producer: &str) -> Option<String> {
    if !valid_path(path) {
        return Some("must name a repository-relative path without traversal or globs".into());
    }
    let Some(task) = def.tasks.get(producer) else {
        return Some(format!("producer `{producer}` is not a task"));
    };
    (!owns(task, path)).then(|| {
        format!("producer {producer} does not own `{path}` in files or new_files; assign the path to that task or name its actual producer")
    })
}

pub fn validate(def: &FleetDefinition) -> Vec<Missing> {
    let mut found = Vec::new();
    for (path, producer) in &def.shared_file_producers {
        let item = format!("shared_file_producers.{path}");
        if let Some(problem) = producer_problem(def, path, producer) {
            found.push(Missing::new(&item, problem));
        }
        if !shared(def, path) {
            found.push(Missing::new(&item, "the path is not in shared_files"));
        }
    }
    criterion_paths(def, &mut found);
    required_paths(def, &mut found);
    conflicting_producers(def, &mut found);
    exports(def, &mut found);
    done_when(def, &mut found);
    found
}

fn exports(def: &FleetDefinition, found: &mut Vec<Missing>) {
    for (id, task) in &def.tasks {
        for (index, path) in task.exports.iter().enumerate() {
            let item = format!("tasks.{id}.exports.{index}");
            if !valid_path(path) {
                found.push(Missing::new(
                    item,
                    format!(
                        "`{path}` must name a repository-relative path without traversal or globs"
                    ),
                ));
            } else if !owns(task, path) {
                found.push(Missing::new(
                    item,
                    format!("task {id} exports `{path}` but does not own it in files or new_files; export only what the task writes"),
                ));
            }
        }
    }
}

const MOST_EXIT_CODE: i32 = 125;

fn done_when(def: &FleetDefinition, found: &mut Vec<Missing>) {
    for (id, task) in &def.tasks {
        let Some(done) = &task.done_when else {
            continue;
        };
        let item = format!("tasks.{id}.done_when");
        if done.checks.is_empty()
            && done.tests_added_under.is_empty()
            && done.files_changed.is_empty()
            && done.exercise.is_none()
        {
            found.push(Missing::new(
                &item,
                "is empty; name at least one check, files_changed, tests_added_under or exercise",
            ));
        }
        done_checks(def, id, done, found);
        for (key, paths) in [
            ("tests_added_under", &done.tests_added_under),
            ("files_changed", &done.files_changed),
        ] {
            for (index, path) in paths.iter().enumerate() {
                done_path(task, id, &format!("{item}.{key}.{index}"), path, found);
            }
        }
        if let Some(exercise) = &done.exercise {
            for (key, text) in [
                ("command", &exercise.command),
                ("expect_output", &exercise.expect_output),
            ] {
                if text.trim().is_empty() {
                    found.push(Missing::new(
                        format!("{item}.exercise.{key}"),
                        "is empty; an exercise names the command to run and the output that shows it worked",
                    ));
                }
            }
        }
    }
}

fn done_checks(
    def: &FleetDefinition,
    id: &str,
    done: &super::definition::DoneWhen,
    found: &mut Vec<Missing>,
) {
    for (index, check) in done.checks.iter().enumerate() {
        let item = format!("tasks.{id}.done_when.checks.{index}");
        if !def.execution.operations.contains_key(&check.operation) {
            let declared: Vec<&str> = def
                .execution
                .operations
                .keys()
                .map(String::as_str)
                .collect();
            found.push(Missing::new(
                format!("{item}.operation"),
                format!(
                    "`{}` is not a declared operation; declare it under execution.operations or use one of: {}",
                    check.operation,
                    if declared.is_empty() { "none declared".into() } else { declared.join(", ") }
                ),
            ));
        }
        if !(0..=MOST_EXIT_CODE).contains(&check.expected_exit) {
            found.push(Missing::new(
                format!("{item}.expected_exit"),
                format!(
                    "`{}` is not an exit code; use 0 to {MOST_EXIT_CODE}",
                    check.expected_exit
                ),
            ));
        }
    }
}

fn done_path(task: &TaskDefinition, id: &str, item: &str, path: &str, found: &mut Vec<Missing>) {
    if !valid_path(path) {
        found.push(Missing::new(
            item,
            format!("`{path}` must name a repository-relative path without traversal or globs"),
        ));
    } else if !owns(task, path) {
        found.push(Missing::new(
            item,
            format!("`{path}` is not in task {id}'s files or new_files, so the task cannot change it; name a path the task owns"),
        ));
    }
}

fn manifest_unit(
    dir: &std::path::Path,
    root: &std::path::Path,
    manifests: &[PackageManifest],
) -> Option<String> {
    let declared = manifests.iter().find(|manifest| {
        std::fs::read_to_string(dir.join(&manifest.file))
            .is_ok_and(|text| text.contains(&manifest.contains))
    });
    if let Some(manifest) = declared {
        return Some(manifest.label.clone());
    }
    let package = std::fs::read_to_string(dir.join("package.json")).ok()?;
    (dir != root || !package.contains("\"workspaces\""))
        .then(|| "package.json workspace member".to_string())
}

fn compile_unit(
    root: &std::path::Path,
    path: &str,
    manifests: &[PackageManifest],
) -> Option<(String, String)> {
    let mut dir = root.join(normal(path));
    while dir.starts_with(root) {
        if dir.is_dir()
            && let Some(kind) = manifest_unit(&dir, root, manifests)
        {
            let relative = dir.strip_prefix(root).ok()?.to_string_lossy().to_string();
            return Some((
                if relative.is_empty() {
                    ".".into()
                } else {
                    relative
                },
                kind,
            ));
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

pub fn compile_unit_warnings(def: &FleetDefinition) -> Vec<Missing> {
    let root = std::path::Path::new(&def.repo);
    let manifests = &def.execution.package_manifests;
    let mut units: BTreeMap<(String, String), BTreeSet<&str>> = BTreeMap::new();
    for (id, task) in def.tasks.iter() {
        for path in task.files.iter().chain(&task.new_files) {
            if let Some(unit) = compile_unit(root, path, manifests) {
                units.entry(unit).or_default().insert(id.as_str());
            }
        }
    }
    units
        .into_iter()
        .filter(|(_, tasks)| tasks.len() > 1)
        .map(|((unit, kind), tasks)| {
            let names: Vec<&str> = tasks.iter().copied().collect();
            Missing::new(
                format!("tasks.{}.files", names[1]),
                format!(
                    "tasks {} all change files in the {kind} `{unit}`; one broken edit stops the build for all of them. Give the unit to one task, or order the tasks with after",
                    names.join(", ")
                ),
            )
        })
        .collect()
}

fn criterion_paths(def: &FleetDefinition, found: &mut Vec<Missing>) {
    for (id, criterion) in &def.criteria {
        for (index, path) in criterion.paths.iter().enumerate() {
            let item = format!("criteria.{id}.paths.{index}");
            if !valid_path(path) {
                found.push(Missing::new(
                    item,
                    format!(
                        "`{path}` must name a repository-relative path without traversal or globs"
                    ),
                ));
            } else if !def.tasks.values().any(|task| owns(task, path)) {
                found.push(Missing::new(
                    item,
                    format!("no task owns `{path}` on criterion {id}'s declared end-to-end path; assign it in a task's files or new_files"),
                ));
            }
        }
    }
}

fn required_paths(def: &FleetDefinition, found: &mut Vec<Missing>) {
    for (id, task) in &def.tasks {
        for (path, producer) in &task.required_paths {
            let item = format!("tasks.{id}.required_paths.{path}");
            if producer == id {
                found.push(Missing::new(
                    &item,
                    "must name another task as the producer",
                ));
            }
            if let Some(problem) = producer_problem(def, path, producer) {
                found.push(Missing::new(&item, problem));
            }
            if let Some(shared_producer) = producer_for(def, path)
                && shared_producer != producer
            {
                found.push(Missing::new(
                    &item,
                    format!("`{path}` has shared producer {shared_producer}, not {producer}"),
                ));
            }
        }
    }
}

fn conflicting_producers(def: &FleetDefinition, found: &mut Vec<Missing>) {
    let entries: Vec<_> = def.shared_file_producers.iter().collect();
    for (index, (path, producer)) in entries.iter().enumerate() {
        for (other_path, other_producer) in &entries[index + 1..] {
            if producer != other_producer
                && (contains(path, other_path) || contains(other_path, path))
            {
                found.push(Missing::new(
                    format!("shared_file_producers.{path}"),
                    format!("producer {producer} conflicts with {other_producer} for overlapping path `{other_path}`; name one producer for the shared hunk"),
                ));
            }
        }
    }
}

pub fn producer_for<'a>(def: &'a FleetDefinition, path: &str) -> Option<&'a str> {
    let mut producer = None;
    for (declared, candidate) in &def.shared_file_producers {
        if !contains(declared, path) {
            continue;
        }
        if !shared(def, declared)
            || producer_problem(def, declared, candidate).is_some()
            || producer.is_some_and(|found| found != candidate)
        {
            return None;
        }
        producer = Some(candidate.as_str());
    }
    producer.filter(|producer| {
        !def.shared_file_producers
            .iter()
            .any(|(declared, candidate)| contains(path, declared) && candidate != producer)
    })
}

fn ancestors<'a>(def: &'a FleetDefinition, start: &str) -> Option<BTreeSet<&'a str>> {
    let mut found = BTreeSet::new();
    let mut queue: Vec<&str> = def
        .tasks
        .get(start)?
        .after
        .iter()
        .map(String::as_str)
        .collect();
    while let Some(next) = queue.pop() {
        let task = def.tasks.get(next)?;
        if found.insert(next) {
            queue.extend(task.after.iter().map(String::as_str));
        }
    }
    Some(found)
}

fn ordered_after(def: &FleetDefinition, later: &str, earlier: &str) -> bool {
    let Some(before) = ancestors(def, later) else {
        return false;
    };
    before.contains(earlier)
        && !before.contains(later)
        && before
            .iter()
            .all(|id| ancestors(def, id).is_some_and(|ancestors| !ancestors.contains(id)))
}

pub fn valid_dependencies(def: &FleetDefinition, after: &[String]) -> bool {
    after.iter().all(|id| {
        ancestors(def, id).is_some_and(|before| {
            !before.contains(id.as_str())
                && before.iter().all(|ancestor| {
                    ancestors(def, ancestor).is_some_and(|ancestors| !ancestors.contains(ancestor))
                })
        })
    })
}

fn resolved(def: &FleetDefinition, overlap: &Overlap) -> bool {
    let (one, other, path) = overlap;
    shared(def, path)
        && (producer_for(def, path).is_some()
            || ordered_after(def, one, other)
            || ordered_after(def, other, one))
}

fn ownership(def: &FleetDefinition) -> Vec<(&str, String)> {
    def.tasks
        .iter()
        .flat_map(|(id, task)| {
            task.files
                .iter()
                .chain(&task.new_files)
                .map(move |path| (id.as_str(), normal(path)))
        })
        .filter(|(_, path)| !path.is_empty())
        .collect()
}

fn overlaps(def: &FleetDefinition, done: &BTreeSet<String>) -> BTreeSet<Overlap> {
    let owned: Vec<(&str, String)> = ownership(def)
        .into_iter()
        .filter(|(id, _)| !done.contains(*id))
        .collect();
    let mut found = BTreeSet::new();
    for (index, (id, path)) in owned.iter().enumerate() {
        for (other, theirs) in &owned[index + 1..] {
            if id == other {
                continue;
            }
            let common = if contains(path, theirs) {
                Some(theirs)
            } else if contains(theirs, path) {
                Some(path)
            } else {
                None
            };
            if let Some(common) = common {
                found.insert(((*id).to_string(), (*other).to_string(), common.clone()));
            }
        }
    }
    found
}

pub fn changed_overlaps(before: &FleetDefinition, after: &FleetDefinition) -> Vec<Missing> {
    changed_overlaps_done(before, after, &BTreeSet::new())
}

pub fn changed_overlaps_done(
    before: &FleetDefinition,
    after: &FleetDefinition,
    done: &BTreeSet<String>,
) -> Vec<Missing> {
    let existing = overlaps(before, done);
    overlaps(after, done)
        .into_iter()
        .filter(|overlap| {
            (!existing.contains(overlap) || resolved(before, overlap))
                && !resolved(after, overlap)
        })
        .map(|(one, other, path)| {
            Missing::new(
                format!("tasks.{other}.files"),
                format!("`{path}` is also owned by task {one}; give it to one task, or list it in shared_files and choose shared_file_producers or an acyclic after order before adding this overlap"),
            )
        })
        .collect()
}

pub fn proposed_owners(
    def: &FleetDefinition,
    (id, task): (&str, TaskDefinition),
    done: &BTreeSet<String>,
) -> BTreeMap<String, Vec<String>> {
    let mut proposed = def.clone();
    proposed.tasks.insert(id.to_string(), task);
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for overlap in overlaps(&proposed, done) {
        let (one, other, path) = &overlap;
        if (one == id || other == id) && !resolved(&proposed, &overlap) {
            let owner = if one == id { other } else { one };
            found.entry(owner.clone()).or_default().push(path.clone());
        }
    }
    found
}

pub fn brief(def: &FleetDefinition, task: &TaskDefinition) -> String {
    let mut lines = Vec::new();
    for (path, producer) in &def.shared_file_producers {
        lines.push(format!("Shared path `{path}`: task {producer} writes the shared hunk; ask that producer for changes rather than holding each other's commits."));
    }
    for (path, producer) in &task.required_paths {
        lines.push(format!(
            "Required path `{path}`: task {producer} is the producer to ask."
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
#[path = "checks_paths_tests.rs"]
pub mod tests;
