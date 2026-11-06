use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use uuid::Uuid;

use super::paths::{first_token, is_none_like, looks_like_path, relativize};

static LABEL_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*(?:[-*]\s+)?(?:\*\*)?(where we are|next up|next|dependencies|branch|worktree|files|open questions|decisions(?:\s*\([^)]*\))?|blockers)(?:\*\*)?\s*:(?:\*\*)?\s*(.*)$",
    )
    .unwrap_or_else(|_| unreachable!("label pattern is a constant"))
});
static ENTRY_START: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[-*]\s+|#{3}\s+)?(?:\*\*)?(\d{4}-\d{2}-\d{2})\b(.*)$")
        .unwrap_or_else(|_| unreachable!("entry pattern is a constant"))
});
static UUID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
        .unwrap_or_else(|_| unreachable!("uuid pattern is a constant"))
});
static SHA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@\s*`?([0-9a-f]{7,40})\b")
        .unwrap_or_else(|_| unreachable!("sha pattern is a constant"))
});
static CRITERIA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ms)^[ \t]*<acceptance-criteria>[ \t]*\n?(.*?)\n?[ \t]*</acceptance-criteria>")
        .unwrap_or_else(|_| unreachable!("criteria pattern is a constant"))
});

#[derive(Debug, Default, Clone)]
pub(super) struct StateOfPlay {
    pub where_we_are: String,
    pub next_up: String,
    pub blockers: String,
    pub open_questions: String,
    pub decisions: String,
    pub dependencies: String,
    pub branch: String,
    pub worktree: String,
    pub files: String,
}

#[derive(Debug, Clone)]
pub(super) struct LogEntry {
    pub date: Option<NaiveDate>,
    pub session: Option<Uuid>,
    pub sha: Option<String>,
    pub summary: String,
    pub body: String,
}

pub(super) fn first_uuid(text: &str) -> Option<Uuid> {
    UUID.find(text)
        .and_then(|found| Uuid::parse_str(found.as_str()).ok())
}

pub(super) fn all_uuids(text: &str) -> Vec<Uuid> {
    UUID.find_iter(text)
        .filter_map(|found| Uuid::parse_str(found.as_str()).ok())
        .collect()
}

fn sections<'a>(body: &'a str, heading: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut start = None;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end();
        let is_h2 = trimmed.starts_with("## ") || trimmed == "##";
        if let Some(begin) = start.filter(|_| is_h2) {
            found.push(&body[begin..offset]);
            start = None;
        }
        if start.is_none()
            && is_h2
            && trimmed[2..]
                .trim()
                .to_ascii_lowercase()
                .starts_with(heading)
        {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    if let Some(begin) = start {
        found.push(&body[begin..]);
    }
    found
}

fn section<'a>(body: &'a str, heading: &str) -> Option<&'a str> {
    sections(body, heading).into_iter().next()
}

pub(super) fn criteria(body: &str) -> Option<String> {
    CRITERIA
        .captures(body)
        .and_then(|found| found.get(1))
        .map(|text| text.as_str().trim().to_owned())
        .filter(|text| !text.is_empty())
}

pub(super) fn state_of_play(body: &str) -> Option<StateOfPlay> {
    let text = section(body, "state of play")?;
    let mut state = StateOfPlay::default();
    let mut current: Option<(String, bool)> = None;
    let mut labelled = false;
    for line in text.lines() {
        let bulleted = line.starts_with(['-', '*']);
        if let Some(found) = LABEL_LINE.captures(line) {
            labelled = true;
            let label = found[1].to_ascii_lowercase();
            let label = label
                .split('(')
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
            let slot = slot_for(&mut state, &label);
            current = if slot.is_empty() {
                slot.push_str(found[2].trim());
                Some((label, bulleted))
            } else {
                None
            };
        } else if current
            .as_ref()
            .is_some_and(|(_, label_bulleted)| *label_bulleted && bulleted)
        {
            current = None;
        } else if let Some((label, _)) = &current {
            let slot = slot_for(&mut state, label);
            slot.push('\n');
            slot.push_str(line.trim_end());
        }
    }
    if !labelled {
        state.where_we_are = text.trim().to_owned();
    }
    for label in [
        "where we are",
        "next",
        "blockers",
        "open questions",
        "decisions",
        "dependencies",
        "branch",
        "worktree",
        "files",
    ] {
        let slot = slot_for(&mut state, label);
        let cleaned = slot.trim().to_owned();
        *slot = if is_none_like(&cleaned) {
            String::new()
        } else {
            cleaned
        };
    }
    Some(state)
}

fn slot_for<'a>(state: &'a mut StateOfPlay, label: &str) -> &'a mut String {
    match label {
        "where we are" => &mut state.where_we_are,
        "next" | "next up" => &mut state.next_up,
        "blockers" => &mut state.blockers,
        "open questions" => &mut state.open_questions,
        "decisions" => &mut state.decisions,
        "dependencies" => &mut state.dependencies,
        "branch" => &mut state.branch,
        "worktree" => &mut state.worktree,
        _ => &mut state.files,
    }
}

pub(super) fn file_list(text: &str, roots: &[String]) -> Vec<String> {
    let backticked: Vec<&str> = text.split('`').skip(1).step_by(2).collect();
    let tokens: Vec<String> = if backticked.is_empty() {
        text.split([',', ';', '\n'])
            .filter_map(|part| first_token(part.trim().trim_start_matches(['-', '*']).trim()))
            .collect()
    } else {
        backticked
            .iter()
            .map(|token| token.trim().to_owned())
            .collect()
    };
    let mut files: Vec<String> = Vec::new();
    for path in tokens
        .iter()
        .filter(|token| looks_like_path(token))
        .filter_map(|token| relativize(token, roots))
        .filter(|path| !path.is_empty())
    {
        if !files.contains(&path) {
            files.push(path);
        }
    }
    files
}

pub(super) fn dependency_slugs(text: &str) -> Vec<String> {
    let backticked: Vec<&str> = text.split('`').skip(1).step_by(2).collect();
    let words: Vec<&str> = if backticked.is_empty() {
        text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')'))
            .collect()
    } else {
        backticked
    };
    let mut slugs: Vec<String> = words
        .into_iter()
        .map(|word| word.trim_matches(|c: char| matches!(c, '.' | '*' | '"' | '\'' | ':')))
        .filter(|word| {
            word.contains('-')
                && word.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                && !word.chars().all(|c| c.is_ascii_digit() || c == '-')
        })
        .map(str::to_owned)
        .collect();
    slugs.sort();
    slugs.dedup();
    slugs
}

pub(super) fn session_log(body: &str) -> Vec<LogEntry> {
    let mut raw: Vec<Vec<&str>> = Vec::new();
    for line in sections(body, "session log")
        .into_iter()
        .flat_map(str::lines)
    {
        if ENTRY_START.is_match(line) {
            raw.push(vec![line]);
        } else if let Some(open) = raw.last_mut() {
            open.push(line);
        }
    }
    raw.into_iter().map(|lines| entry_from(&lines)).collect()
}

fn entry_from(lines: &[&str]) -> LogEntry {
    let header = lines.first().copied().unwrap_or_default();
    let found = ENTRY_START.captures(header);
    let date = found
        .as_ref()
        .and_then(|found| NaiveDate::parse_from_str(&found[1], "%Y-%m-%d").ok());
    let rest = found
        .as_ref()
        .map_or("", |found| found.get(2).map_or("", |m| m.as_str()));
    let sha = SHA.captures(header).map(|found| found[1].to_owned());
    let mut summary = UUID.replace_all(rest, "").into_owned();
    summary = SHA.replace_all(&summary, "").into_owned();
    let mut summary = clean_summary(&summary);
    if summary.is_empty() {
        summary = lines
            .iter()
            .skip(1)
            .map(|line| clean_summary(line))
            .find(|line| !line.is_empty())
            .unwrap_or_else(|| "(no text)".to_owned());
    }
    LogEntry {
        date,
        session: first_uuid(header),
        sha,
        summary,
        body: lines.join("\n").trim().to_owned(),
    }
}

fn clean_summary(text: &str) -> String {
    let stripped = text
        .replace("()", "")
        .replace("(session )", "")
        .replace("(session)", "");
    stripped
        .trim()
        .trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, '-' | '*' | ':' | '\u{2014}' | '\u{2013}' | '~' | ',')
        })
        .trim_end_matches('*')
        .trim()
        .to_owned()
}
