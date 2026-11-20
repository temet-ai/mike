use std::collections::BTreeSet;

use serde_json::Value;

use crate::compress::file_reads::glob::glob_match;

const MIN_RULE_CHARS: usize = 25;
const MAX_RULE_CHARS: usize = 600;
const FILLER: [&str; 24] = [
    "the", "and", "for", "not", "use", "you", "are", "with", "from", "that", "this", "its", "any",
    "all", "but", "into", "has", "have", "when", "one", "each", "every", "than", "then",
];
const MAX_FILE_BYTES: usize = 60_000;
const MAX_RULE_FILES: usize = 12;
const NORMATIVE: [&str; 14] = [
    "never",
    "must",
    "always",
    "do not",
    "don't",
    "don\u{2019}t",
    "forbid",
    "forbids",
    "forbidden",
    "refuse",
    "refuses",
    "reject",
    "rejects",
    "rejected",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub source: String,
    pub text: String,
    pub scope: Vec<String>,
}

const COMMIT_WORDS: [&str; 8] = [
    "commit",
    "commits",
    "committing",
    "committed",
    "push",
    "pushes",
    "pushing",
    "pushed",
];
const WRITE_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

impl Rule {
    pub fn is_obligation(&self) -> bool {
        let lower = self.text.to_lowercase();
        has_word(&lower, "must") || has_word(&lower, "always")
    }

    pub fn is_about_commits(&self) -> bool {
        let lower = self.text.to_lowercase();
        COMMIT_WORDS.iter().any(|w| has_word(&lower, w))
    }
}

fn is_commit_or_push(tool_name: &str, command: &str) -> bool {
    tool_name == "Bash" && (command.contains("git commit") || command.contains("git push"))
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

fn has_word(lower: &str, word: &str) -> bool {
    lower.match_indices(word).any(|(at, _)| {
        let before = lower[..at].chars().next_back();
        let after = lower[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

fn is_rule(text: &str) -> bool {
    let length = text.chars().count();
    let lower = text.to_lowercase();
    (MIN_RULE_CHARS..=MAX_RULE_CHARS).contains(&length)
        && NORMATIVE.iter().any(|w| has_word(&lower, w))
}

fn continues(line: &str) -> bool {
    let trimmed = line.trim_start();
    !trimmed.is_empty()
        && bullet(line).is_none()
        && !trimmed.starts_with('#')
        && !trimmed.starts_with('|')
        && !trimmed.starts_with("```")
}

fn items(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open = false;
    for line in body.lines() {
        if let Some(start) = bullet(line) {
            out.push(start.to_owned());
            open = true;
        } else if open && continues(line) {
            if let Some(last) = out.last_mut() {
                last.push(' ');
                last.push_str(line.trim());
            }
        } else {
            open = false;
        }
    }
    out
}

pub fn parse_rules(source: &str, text: &str) -> Vec<Rule> {
    let (scope, body) = frontmatter_scope(text);
    items(body)
        .into_iter()
        .filter(|line| is_rule(line))
        .map(|line| Rule {
            source: source.to_owned(),
            text: line,
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
    pub full: Vec<String>,
    pub paths: Vec<String>,
    pub committed: bool,
}

const LINE_CHARS: usize = 200;
const LINES_KEPT: usize = 15;
const MATCHED_LINE_CHARS: usize = 4_000;

pub fn cut_at_whitespace(text: &str, max: usize) -> &str {
    let Some((end, _)) = text.char_indices().nth(max) else {
        return text;
    };
    let head = &text[..end];
    if text[end..].starts_with(char::is_whitespace) {
        return head.trim_end();
    }
    head.rfind(char::is_whitespace)
        .map_or(head, |at| head[..at].trim_end())
}

impl TurnTools {
    pub fn from_calls<'a>(calls: impl IntoIterator<Item = (&'a str, &'a Value)>) -> Self {
        let mut tools = Self::default();
        for (name, input) in calls {
            let path = ["file_path", "path", "notebook_path"]
                .iter()
                .find_map(|k| input.get(*k).and_then(Value::as_str));
            let command = input.get("command").and_then(Value::as_str);
            let detail = command.or(path).unwrap_or_default();
            tools
                .lines
                .push(format!("{name}: {}", cut_at_whitespace(detail, LINE_CHARS)));
            tools.full.push(format!("{name}: {detail}"));
            if WRITE_TOOLS.contains(&name) {
                tools.paths.extend(path.map(str::to_owned));
            }
            tools.committed |= command.is_some_and(|c| is_commit_or_push(name, c));
        }
        tools
    }

    pub fn writes_only_outside(&self, root: &str) -> bool {
        let root = std::path::Path::new(root);
        !self.paths.is_empty()
            && !self
                .paths
                .iter()
                .any(|p| std::path::Path::new(p).starts_with(root))
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
        .filter(|w| w.len() >= 3 && !FILLER.contains(w))
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

pub fn listed_lines(turn: &TurnTools, rules: &[&Rule]) -> Vec<String> {
    let wanted: BTreeSet<String> = rules.iter().flat_map(|rule| tokens(&rule.text)).collect();
    let matched: Vec<bool> = turn
        .full
        .iter()
        .map(|line| tokens(line).iter().any(|token| wanted.contains(token)))
        .collect();
    let mut picked: Vec<usize> = (0..matched.len())
        .filter(|at| matched[*at])
        .take(LINES_KEPT)
        .collect();
    let spare = LINES_KEPT - picked.len();
    picked.extend((0..matched.len()).filter(|at| !matched[*at]).take(spare));
    picked.sort_unstable();
    picked
        .into_iter()
        .map(|at| {
            let text = if matched[at] {
                cut_at_whitespace(&turn.full[at], MATCHED_LINE_CHARS)
            } else {
                turn.lines[at].as_str()
            };
            format!("T{:02}| {text}", at + 1)
        })
        .collect()
}
