use std::collections::BTreeSet;
use std::path::Path;

use super::checks::{Missing, normal};
use super::definition::FleetDefinition;

fn has_glob(path: &str) -> bool {
    path.contains(['*', '?', '['])
}

fn declared_new(def: &FleetDefinition, id: &str) -> BTreeSet<String> {
    def.tasks
        .get(id)
        .map(|task| task.new_files.iter().map(|f| normal(f)).collect())
        .unwrap_or_default()
}

pub fn absent_task_files(def: &FleetDefinition, repo: &Path) -> BTreeSet<String> {
    def.tasks
        .values()
        .flat_map(|task| task.files.iter().map(|f| normal(f)))
        .filter(|file| !file.is_empty() && file != "." && !has_glob(file))
        .filter(|file| !repo.join(file).exists())
        .collect()
}

pub fn unlisted_files(def: &FleetDefinition, absent: &BTreeSet<String>) -> Vec<Missing> {
    let mut found = Vec::new();
    for (id, task) in &def.tasks {
        let new = declared_new(def, id);
        let mut reported = BTreeSet::new();
        for file in task.files.iter().map(|f| normal(f)) {
            if absent.contains(&file) && !new.contains(&file) && reported.insert(file.clone()) {
                found.push(Missing::new(
                    format!("tasks.{id}.files"),
                    format!(
                        "`{file}` is not in the repository; correct the path, or list it in the task's new_files if the task creates it"
                    ),
                ));
            }
        }
    }
    found
}
