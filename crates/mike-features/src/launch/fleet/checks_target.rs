use std::collections::{BTreeMap, BTreeSet};

use super::checks::Missing;
use super::definition::FleetDefinition;

fn bare(word: &str) -> &str {
    word.trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | '(' | ')' | ',' | ';'))
        .trim_end_matches(['.', ':'])
}

pub fn named_dirs(text: &str, settings: &[String]) -> BTreeSet<String> {
    let words: Vec<&str> = text.split_whitespace().map(bare).collect();
    let mut dirs = BTreeSet::new();
    for (at, word) in words.iter().enumerate() {
        for setting in settings {
            if let Some(dir) = word
                .strip_prefix(setting.as_str())
                .and_then(|rest| rest.strip_prefix('='))
            {
                dirs.insert(bare(dir).to_string());
            } else if word == setting
                && setting.starts_with('-')
                && let Some(next) = words.get(at + 1)
            {
                dirs.insert((*next).to_string());
            }
        }
    }
    dirs.retain(|dir| !dir.is_empty());
    dirs
}

pub fn per_task_targets(def: &FleetDefinition) -> Vec<Missing> {
    let settings = &def.execution.build_directory_settings;
    let check = named_dirs(&def.check.command, settings);
    let mut by_dir: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for (id, task) in &def.tasks {
        for dir in named_dirs(&task.brief, settings)
            .into_iter()
            .chain(named_dirs(&task.goal, settings))
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
            "the briefs name {} build directories: {}. Each directory starts its build cold, and workers that start together build at the same time. Name one shared directory for every task, or leave it to the project file (rules: one build directory)",
            by_dir.len(),
            named.join("; ")
        ),
    )]
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::launch::fleet::definition::TaskDefinition;

    fn fleet(briefs: &[(&str, &str)]) -> FleetDefinition {
        let mut def = FleetDefinition::default();
        def.execution.build_directory_settings =
            vec!["BUILD_TARGET_DIR".into(), "--target-dir".into()];
        def.check.command = "BUILD_TARGET_DIR=target/fleet-check builder check --workspace".into();
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
                "Test with `BUILD_TARGET_DIR=target/fleet-T1 builder nextest run -p gg-gitlab`.",
            ),
            (
                "T2",
                "Run builder check --target-dir target/fleet-T2 -p gg-slack",
            ),
            ("T3", "Use BUILD_TARGET_DIR=target/fleet-T1 too."),
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
    fn a_project_names_its_own_setting_and_a_bare_variable_name_is_no_directory() {
        let settings = vec!["BUILD_DIR".to_string(), "--out".to_string()];
        let found = named_dirs(
            "BUILD_DIR=out/a; --out out/b --out=out/c, the BUILD_DIR variable",
            &settings,
        );
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            ["out/a", "out/b", "out/c"]
        );
        assert!(named_dirs("BUILD_DIR=out/a", &[]).is_empty());
    }

    #[test]
    fn one_shared_directory_and_the_check_commands_own_directory_pass() {
        let def = fleet(&[
            (
                "T1",
                "BUILD_TARGET_DIR=target/fleet builder nextest run -p a",
            ),
            (
                "T2",
                "BUILD_TARGET_DIR=target/fleet builder test -p b; the check uses BUILD_TARGET_DIR=target/fleet-check",
            ),
            ("T3", "no build directory named"),
        ]);
        assert!(per_task_targets(&def).is_empty());
    }
}
