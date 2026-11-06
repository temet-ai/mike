use std::collections::{BTreeMap, BTreeSet};

use super::checks::Missing;
use super::definition::FleetDefinition;

const MARKERS: [&str; 2] = ["CARGO_TARGET_DIR=", "--target-dir="];

fn bare(word: &str) -> &str {
    word.trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | '(' | ')' | ',' | ';'))
        .trim_end_matches(['.', ':'])
}

pub(super) fn named_dirs(text: &str) -> BTreeSet<String> {
    let words: Vec<&str> = text.split_whitespace().map(bare).collect();
    let mut dirs = BTreeSet::new();
    for (at, word) in words.iter().enumerate() {
        if let Some(dir) = MARKERS.iter().find_map(|m| word.strip_prefix(m)) {
            dirs.insert(bare(dir).to_string());
        } else if *word == "--target-dir"
            && let Some(next) = words.get(at + 1)
        {
            dirs.insert((*next).to_string());
        }
    }
    dirs.retain(|dir| !dir.is_empty());
    dirs
}

pub fn per_task_targets(def: &FleetDefinition) -> Vec<Missing> {
    let check = named_dirs(&def.check.command);
    let mut by_dir: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for (id, task) in &def.tasks {
        for dir in named_dirs(&task.brief)
            .into_iter()
            .chain(named_dirs(&task.goal))
        {
            if !check.contains(&dir) {
                by_dir.entry(dir).or_default().insert(id);
            }
        }
    }
    if by_dir.len() < 2 {
        return Vec::new();
    }
    let named: Vec<String> = by_dir
        .iter()
        .map(|(dir, ids)| {
            format!(
                "`{dir}` ({})",
                ids.iter().copied().collect::<Vec<_>>().join(", ")
            )
        })
        .collect();
    vec![Missing::new(
        "tasks",
        format!(
            "the briefs name {} cargo build directories: {}. Each directory compiles the workspace cold, and workers that start together compile it at the same time. Name one shared directory for every task, or leave it to the project file (rules: one build directory)",
            by_dir.len(),
            named.join("; ")
        ),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::fleet::definition::TaskDefinition;

    fn fleet(briefs: &[(&str, &str)]) -> FleetDefinition {
        let mut def = FleetDefinition::default();
        def.check.command = "CARGO_TARGET_DIR=target/fleet-check cargo check --workspace".into();
        for (id, brief) in briefs {
            def.tasks.insert(
                (*id).into(),
                TaskDefinition {
                    brief: (*brief).into(),
                    ..TaskDefinition::default()
                },
            );
        }
        def
    }

    #[test]
    fn briefs_that_give_each_task_its_own_build_directory_are_refused() {
        let def = fleet(&[
            (
                "T1",
                "Test with `CARGO_TARGET_DIR=target/fleet-T1 cargo nextest run -p gg-gitlab`.",
            ),
            (
                "T2",
                "Run cargo check --target-dir target/fleet-T2 -p gg-slack",
            ),
            ("T3", "Use CARGO_TARGET_DIR=target/fleet-T1 too."),
        ]);
        let found = per_task_targets(&def);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].problem.contains("`target/fleet-T1` (T1, T3)"),
            "{}",
            found[0].problem
        );
        assert!(
            found[0].problem.contains("`target/fleet-T2` (T2)"),
            "{}",
            found[0].problem
        );
    }

    #[test]
    fn one_shared_directory_and_the_check_commands_own_directory_pass() {
        let def = fleet(&[
            ("T1", "CARGO_TARGET_DIR=target/fleet cargo nextest run -p a"),
            (
                "T2",
                "CARGO_TARGET_DIR=target/fleet cargo test -p b; the check uses CARGO_TARGET_DIR=target/fleet-check",
            ),
            ("T3", "no build directory named"),
        ]);
        assert!(per_task_targets(&def).is_empty());
    }
}
