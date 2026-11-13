use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use super::definition::FleetDefinition;
use super::gate_queue::{commits_since, said};
use super::watch::git;

const SEAT_TRAILER: &str = "--format=%(trailers:key=Fleet-Seat,valueonly)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    Defines(String),
    Names(String),
    Located(String),
    Touches(String),
}

impl Evidence {
    pub fn reason(&self) -> String {
        match self {
            Self::Defines(name) => format!("defines `{name}`, which the failing output names"),
            Self::Names(name) => {
                format!("is the only commit that adds `{name}`, which the failing output names")
            }
            Self::Located(file) => format!(
                "changes `{file}`, where the failing output places an error, and adds a name the output quotes"
            ),
            Self::Touches(file) => format!("touches `{file}`, which the failing output names"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspect {
    pub commit: String,
    pub seat: String,
    pub since: usize,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blame {
    Suspects(Vec<Suspect>),
    NoMatch { since: usize },
}

#[derive(Debug, Clone, Default)]
pub struct Change {
    pub files: Vec<String>,
    pub words: HashSet<String>,
    pub defines: HashSet<String>,
}

impl Change {
    pub fn from_diff(files: &str, diff: &str) -> Self {
        let added: Vec<&str> = diff
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .map(|line| &line[1..])
            .collect();
        let defines = definition().map_or_else(HashSet::new, |pattern| {
            added
                .iter()
                .flat_map(|line| pattern.captures_iter(line))
                .filter_map(|found| found.get(1).map(|name| name.as_str().to_string()))
                .collect()
        });
        Self {
            files: files.lines().map(str::to_string).collect(),
            words: added
                .iter()
                .flat_map(|line| words(line))
                .map(str::to_string)
                .collect(),
            defines,
        }
    }

    fn located(&self, output: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|file| at_a_line(output, file))
            .map(String::as_str)
    }
}

fn definition() -> Option<&'static Regex> {
    static DEFINITION: OnceLock<Option<Regex>> = OnceLock::new();
    DEFINITION
        .get_or_init(|| {
            Regex::new(
                r"(?i)\b(?:struct|enum|trait|type|fn|mod|const|static|table|view|index|function)\s+(?:if\s+not\s+exists\s+)?(?:[a-z_][a-z0-9_]*\.)?([a-z_][a-z0-9_]*)",
            )
            .ok()
        })
        .as_ref()
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
}

fn distinctive(word: &str) -> bool {
    let camel = word
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0].is_ascii_lowercase() && pair[1].is_ascii_uppercase());
    let snake = word.trim_matches('_').contains('_');
    word.len() >= 5
        && !word.starts_with(|c: char| c.is_ascii_digit())
        && word.chars().any(|c| c.is_ascii_lowercase())
        && (camel || snake)
}

fn at_a_line(output: &str, file: &str) -> bool {
    output.match_indices(file).any(|(at, _)| {
        let rest = &output[at + file.len()..];
        rest.strip_prefix(':')
            .is_some_and(|tail| tail.starts_with(|c: char| c.is_ascii_digit()))
    })
}

pub fn evidence(changes: &[Change], output: &str) -> Vec<(usize, Evidence)> {
    let names: BTreeSet<&str> = words(output).filter(|word| distinctive(word)).collect();
    let mut found: Vec<Option<Evidence>> = vec![None; changes.len()];
    for name in &names {
        for (index, change) in changes.iter().enumerate() {
            if change.defines.contains(*name) {
                found[index].get_or_insert_with(|| Evidence::Defines((*name).to_string()));
            }
        }
    }
    for (index, change) in changes.iter().enumerate() {
        if let Some(file) = change.located(output)
            && names.iter().any(|name| change.words.contains(*name))
        {
            found[index].get_or_insert_with(|| Evidence::Located(file.to_string()));
        }
    }
    for name in &names {
        let naming: Vec<usize> = (0..changes.len())
            .filter(|index| changes[*index].words.contains(*name))
            .collect();
        if let [only] = naming[..] {
            found[only].get_or_insert_with(|| Evidence::Names((*name).to_string()));
        }
    }
    if found.iter().all(Option::is_none) {
        return touched(changes, output).into_iter().collect();
    }
    found
        .into_iter()
        .enumerate()
        .filter_map(|(index, evidence)| evidence.map(|evidence| (index, evidence)))
        .collect()
}

fn touched(changes: &[Change], output: &str) -> Option<(usize, Evidence)> {
    changes.iter().enumerate().find_map(|(index, change)| {
        change
            .files
            .iter()
            .find(|file| output.contains(file.as_str()))
            .map(|file| (index, Evidence::Touches(file.clone())))
    })
}

async fn change_of(repo: &str, commit: &str) -> Change {
    let files = [
        "diff-tree",
        "--no-commit-id",
        "--name-only",
        "-r",
        "--root",
        commit,
    ];
    let diff = [
        "show",
        "--format=",
        "--no-color",
        "--no-ext-diff",
        "-U0",
        commit,
    ];
    Change::from_diff(
        &said(git(repo, &files, None).await).unwrap_or_default(),
        &said(git(repo, &diff, None).await).unwrap_or_default(),
    )
}

pub async fn blame(
    def: &FleetDefinition,
    passed: Option<&str>,
    failed: &str,
    output: &str,
) -> Option<Blame> {
    let mut commits = Vec::new();
    for commit in commits_since(&def.repo, passed, failed).await {
        if let Some(seat) = fleet_seat(def, &commit).await {
            commits.push((commit, seat));
        }
    }
    let since = commits.len();
    if since == 0 {
        return None;
    }
    let mut changes = Vec::with_capacity(since);
    for (commit, _) in &commits {
        changes.push(change_of(&def.repo, commit).await);
    }
    let suspects: Vec<Suspect> = evidence(&changes, output)
        .into_iter()
        .map(|(index, evidence)| Suspect {
            commit: commits[index].0.clone(),
            seat: commits[index].1.clone(),
            since,
            evidence,
        })
        .collect();
    Some(if suspects.is_empty() {
        Blame::NoMatch { since }
    } else {
        Blame::Suspects(suspects)
    })
}

async fn fleet_seat(def: &FleetDefinition, commit: &str) -> Option<String> {
    let trailer = said(git(&def.repo, &["log", "-1", SEAT_TRAILER, commit], None).await)?;
    def.tasks.values().find_map(|task| {
        let seat = task.seat.as_deref()?;
        let session = task.session.as_deref()?;
        (trailer.trim() == format!("{seat} ({session})")).then(|| seat.to_string())
    })
}

#[cfg(test)]
#[path = "gate_blame_tests.rs"]
pub mod tests;
