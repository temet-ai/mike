use super::checks::Missing;
use super::definition::{FleetDefinition, TaskDefinition};

const REMOVAL_FILES: usize = 20;
const LIVE_WORDS: [&str; 4] = ["live", "served", "running app", "in the app"];
const SEED_WORDS: [&str; 7] = ["seed", "fixture", "demo", "sample", "mock", "stub", "story"];
const REMOVAL_WORDS: [&str; 4] = ["remove", "delete", "retire", "rip out"];

pub(super) fn scope(def: &FleetDefinition) -> Vec<Missing> {
    let mut found = Vec::new();
    for (id, task) in &def.tasks {
        if let Some(problem) = live_without_seed(task) {
            found.push(Missing::new(format!("tasks.{id}.brief"), problem));
        }
        if let Some(problem) = removal_too_wide(task) {
            found.push(Missing::new(format!("tasks.{id}"), problem));
        }
    }
    found
}

fn says(text: &str, words: &[&str]) -> bool {
    let text = text.to_lowercase();
    words.iter().any(|word| {
        text.match_indices(word).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + word.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
    })
}

pub(super) fn live_without_seed(task: &TaskDefinition) -> Option<String> {
    (says(&task.brief, &LIVE_WORDS) && !says(&task.brief, &SEED_WORDS)).then(|| {
        "the brief asks for a live exercise but names nothing that makes it appear; name the seed, fixture or demo data that shows it, and which task produces it (this task, if nobody else)".to_string()
    })
}

pub(super) fn removal_too_wide(task: &TaskDefinition) -> Option<String> {
    let files = task.files.len() + task.new_files.len();
    (files > REMOVAL_FILES && says(&task.brief, &REMOVAL_WORDS)).then(|| {
        format!(
            "a removal across {files} paths in one task leaves the shared tree uncompilable until it all lands; split it into slices that each compile on their own (replace, then delete), one task each, with at most {REMOVAL_FILES} paths"
        )
    })
}

#[cfg(test)]
#[path = "checks_scope_tests.rs"]
mod tests;
