use std::collections::HashSet;

use super::checks::normal;
use super::commit_paths::covers;
use super::definition::FleetDefinition;
use super::watch::git;

const CRATE_ROOTS: [&str; 3] = ["mod.rs", "lib.rs", "main.rs"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Unfiled {
    pub(super) declared_in: String,
    pub(super) module: String,
    pub(super) expected: [String; 2],
}

fn strip_visibility(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("pub") else {
        return text;
    };
    if let Some(inner) = rest.strip_prefix('(') {
        return inner
            .split_once(')')
            .map_or(text, |(_, after)| after.trim_start());
    }
    if rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        text
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn module_name(segment: &str) -> Option<&str> {
    let rest = strip_visibility(segment.trim()).strip_prefix("mod")?;
    let name = rest.strip_prefix(char::is_whitespace)?.trim();
    is_identifier(name).then_some(name)
}

fn strip_attributes(line: &str) -> (bool, &str) {
    let mut redirected = false;
    let mut rest = line;
    while rest.starts_with("#[") {
        let Some((attribute, after)) = rest.split_once(']') else {
            break;
        };
        redirected |= attribute.starts_with("#[path");
        rest = after.trim_start();
    }
    (redirected, rest)
}

pub(super) fn declared(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut redirected = false;
    for line in source.lines() {
        if line.starts_with(char::is_whitespace) || line.starts_with("//") {
            continue;
        }
        let (path_attribute, code) = strip_attributes(line);
        redirected |= path_attribute;
        for segment in code.split(';') {
            let Some(name) = module_name(segment) else {
                continue;
            };
            if !redirected {
                found.push(name.to_string());
            }
            redirected = false;
        }
    }
    found
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

pub(super) fn module_files(file: &str, module: &str) -> [String; 2] {
    let (dir, name) = file.rsplit_once('/').unwrap_or(("", file));
    let home = if CRATE_ROOTS.contains(&name) {
        dir.to_string()
    } else {
        join(dir, name.strip_suffix(".rs").unwrap_or(name))
    };
    [
        join(&home, &format!("{module}.rs")),
        join(&join(&home, module), "mod.rs"),
    ]
}

pub(super) fn unfiled(
    file: &str,
    before: Option<&str>,
    after: &str,
    tree: &HashSet<String>,
) -> Vec<Unfiled> {
    let already: HashSet<String> = before
        .map(declared)
        .unwrap_or_default()
        .into_iter()
        .collect();
    declared(after)
        .into_iter()
        .filter(|module| !already.contains(module))
        .map(|module| Unfiled {
            declared_in: file.to_string(),
            expected: module_files(file, &module),
            module,
        })
        .filter(|missing| !missing.expected.iter().any(|path| tree.contains(path)))
        .collect()
}

fn listing_length(listed: &[String], expected: &[String; 2]) -> Option<usize> {
    listed
        .iter()
        .filter(|prefix| {
            expected
                .iter()
                .any(|path| covers(&[(*prefix).clone()], path))
        })
        .map(String::len)
        .max()
}

fn owning_task<'a>(def: &'a FleetDefinition, expected: &[String; 2]) -> Option<(&'a str, &'a str)> {
    def.tasks
        .iter()
        .filter_map(|(id, task)| {
            let listed: Vec<String> = task
                .files
                .iter()
                .chain(&task.new_files)
                .map(|file| normal(file))
                .collect();
            let length = listing_length(&listed, expected)?;
            Some((length, id.as_str(), task.name.as_str()))
        })
        .max_by_key(|(length, _, _)| *length)
        .map(|(_, id, seat)| (id, seat))
}

pub(super) fn warning(def: &FleetDefinition, missing: &Unfiled) -> String {
    let [file, folder] = &missing.expected;
    let owner = owning_task(def, &missing.expected).map_or_else(String::new, |(task, seat)| {
        format!(" Task {task} ({seat}) lists it: commit it with this file, or after it lands take the declaration out.")
    });
    format!(
        "{} declares `mod {};` but neither {file} nor {folder} is in this commit or in the tree it lands on, so the branch does not build at this commit.{owner}",
        missing.declared_in, missing.module
    )
}

async fn text_at(repo: &str, commit: &str, path: &str) -> Option<String> {
    let spec = format!("{commit}:{path}");
    let bytes = git(repo, &["show", &spec], None).await?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

async fn files_in(repo: &str, tree: &str) -> Option<HashSet<String>> {
    let listing = git(repo, &["ls-tree", "-r", "--name-only", "-z", tree], None).await?;
    Some(
        listing
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect(),
    )
}

pub(super) async fn run(
    def: &FleetDefinition,
    (tree, head): (&str, &str),
    paths: &[String],
) -> Vec<String> {
    let sources: Vec<&String> = paths.iter().filter(|p| p.ends_with(".rs")).collect();
    if sources.is_empty() {
        return Vec::new();
    }
    let Some(files) = files_in(&def.repo, tree).await else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    for path in sources {
        let Some(after) = text_at(&def.repo, tree, path).await else {
            continue;
        };
        let before = text_at(&def.repo, head, path).await;
        for missing in unfiled(path, before.as_deref(), &after, &files) {
            warnings.push(warning(def, &missing));
        }
    }
    warnings
}
