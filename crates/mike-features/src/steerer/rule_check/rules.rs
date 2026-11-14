use std::collections::BTreeSet;

use serde_json::Value;

use crate::compress::file_reads::glob::glob_match;

const MIN_RULE_CHARS: usize = 25;
const MAX_RULE_CHARS: usize = 320;
const MAX_FILE_BYTES: usize = 60_000;
const MAX_RULE_FILES: usize = 12;
const NORMATIVE: [&str; 9] = [
    "never", "must", "always", "do not", "don't", "forbid", "refuse", "reject", "only",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub source: String,
    pub text: String,
    pub scope: Vec<String>,
}

fn frontmatter_scope(text: &str) -> (Vec<String>, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (Vec::new(), text);
    };
    let Some((front, body)) = rest.split_once("\n---") else {
        return (Vec::new(), text);
    };
    let mut in_paths = false;
    let mut scope = Vec::new();
    for line in front.lines() {
        let trimmed = line.trim();
        if let Some(inline) = trimmed.strip_prefix("paths:") {
            in_paths = true;
            scope.extend(
                inline
                    .trim()
                    .trim_matches(['[', ']'])
                    .split(',')
                    .map(|g| g.trim().trim_matches(['"', '\'']).to_owned())
                    .filter(|g| !g.is_empty()),
            );
        } else if in_paths && let Some(item) = trimmed.strip_prefix("- ") {
            scope.push(item.trim().trim_matches(['"', '\'']).to_owned());
        } else if !trimmed.is_empty() {
            in_paths = false;
        }
    }
    (scope, body)
}

fn bullet(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some(rest.trim());
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    line.get(digits..)
        .filter(|_| digits > 0)
        .and_then(|rest| rest.strip_prefix(". "))
        .map(str::trim)
}

fn is_rule(text: &str) -> bool {
    let length = text.chars().count();
    let lower = text.to_lowercase();
    (MIN_RULE_CHARS..=MAX_RULE_CHARS).contains(&length)
        && NORMATIVE.iter().any(|w| lower.contains(w))
}

pub fn parse_rules(source: &str, text: &str) -> Vec<Rule> {
    let (scope, body) = frontmatter_scope(text);
    body.lines()
        .filter_map(bullet)
        .filter(|line| is_rule(line))
        .map(|line| Rule {
            source: source.to_owned(),
            text: line.to_owned(),
            scope: scope.clone(),
        })
        .collect()
}

pub async fn load(checkout: &str) -> Vec<Rule> {
    let root = std::path::Path::new(checkout);
    let mut files = vec![root.join("AGENTS.md"), root.join("CLAUDE.md")];
    if let Ok(mut dir) = tokio::fs::read_dir(root.join(".claude/rules")).await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            if entry.path().extension().is_some_and(|e| e == "md") {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    files.truncate(MAX_RULE_FILES);
    let mut rules = Vec::new();
    for path in files {
        let Ok(text) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        if text.len() > MAX_FILE_BYTES {
            continue;
        }
        let name = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        rules.extend(parse_rules(&name, &text));
    }
    rules
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnTools {
    pub lines: Vec<String>,
    pub paths: Vec<String>,
}

const LINE_CHARS: usize = 200;
const LINES_KEPT: usize = 15;

impl TurnTools {
    pub fn from_calls<'a>(calls: impl IntoIterator<Item = (&'a str, &'a Value)>) -> Self {
        let mut tools = Self::default();
        for (name, input) in calls {
            let path = ["file_path", "path", "notebook_path"]
                .iter()
                .find_map(|k| input.get(*k).and_then(Value::as_str));
            let command = input.get("command").and_then(Value::as_str);
            let detail = command.or(path).unwrap_or_default();
            let cut: String = detail.chars().take(LINE_CHARS).collect();
            tools.lines.push(format!("{name}: {cut}"));
            tools.paths.extend(path.map(str::to_owned));
        }
        tools
    }

    fn bag(&self) -> BTreeSet<String> {
        self.lines
            .iter()
            .flat_map(|line| tokens(line))
            .chain(self.paths.iter().flat_map(|p| tokens(p)))
            .collect()
    }
}

fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(|w| w.trim_end_matches('s').to_owned())
        .collect()
}

fn relevance(rule: &Rule, bag: &BTreeSet<String>, paths: &[String]) -> usize {
    let overlap = tokens(&rule.text)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .intersection(bag)
        .count();
    let scoped = rule
        .scope
        .iter()
        .any(|glob| paths.iter().any(|p| glob_match(glob, p)));
    overlap + if scoped { 3 } else { 0 }
}

pub fn relevant<'a>(rules: &'a [Rule], turn: &TurnTools, limit: usize) -> Vec<&'a Rule> {
    let bag = turn.bag();
    let mut scored: Vec<(usize, &Rule)> = rules
        .iter()
        .map(|rule| (relevance(rule, &bag, &turn.paths), rule))
        .filter(|(score, _)| *score >= 2)
        .collect();
    scored.sort_by_key(|b| std::cmp::Reverse(b.0));
    scored.into_iter().take(limit).map(|(_, r)| r).collect()
}

pub fn summary_lines(turn: &TurnTools) -> Vec<&str> {
    turn.lines
        .iter()
        .take(LINES_KEPT)
        .map(String::as_str)
        .collect()
}
