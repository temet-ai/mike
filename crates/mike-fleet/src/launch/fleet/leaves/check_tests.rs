use std::sync::LazyLock;

use regex::Regex;

use crate::launch::fleet::watch::git;

pub const MAX_NAMES: usize = 60;
const NAME_CHARS: usize = 160;
const MAX_FILES: usize = 8;

static QUOTED: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r#"^\s*(?:test|it|describe|let%test|let%expect_test)(?:\.\w+)?\s*\(?\s*["'`]([^"'`]+)"#,
    )
    .ok()
});
static NAMED_FN: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)|^\s*def\s+(test_\w+)").ok()
});

fn looks_like_file(token: &str) -> bool {
    let token = token.trim_matches(|c| matches!(c, '"' | '\'' | '`' | ',' | ';'));
    token.contains('.')
        && !token.starts_with('-')
        && !token.starts_with('/')
        && !token.contains("..")
        && token
            .rsplit('.')
            .next()
            .is_some_and(|ext| ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn clean(token: &str) -> String {
    token
        .trim_matches(|c| matches!(c, '"' | '\'' | '`' | ',' | ';'))
        .to_owned()
}

pub fn candidate_files(command: &str, tests_changed: &[String]) -> Vec<String> {
    let from_command = command.split_whitespace().filter(|t| looks_like_file(t));
    let from_changed = tests_changed
        .iter()
        .filter_map(|entry| entry.split_whitespace().next())
        .filter(|t| looks_like_file(t));
    let mut files: Vec<String> = Vec::new();
    for token in from_command.chain(from_changed).map(clean) {
        if !files.contains(&token) {
            files.push(token);
        }
    }
    files.truncate(MAX_FILES);
    files
}

fn is_test_attribute(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("#[") && line.contains("test")
}

pub fn names_in(source: &str) -> Vec<String> {
    let (Some(quoted), Some(named)) = (QUOTED.as_ref(), NAMED_FN.as_ref()) else {
        return Vec::new();
    };
    let mut previous = "";
    let mut out = Vec::new();
    for line in source.lines() {
        if let Some(name) = quoted.captures(line).and_then(|c| c.get(1)) {
            out.push(name.as_str().to_owned());
        } else if let Some(caps) = named.captures(line) {
            let fn_name = caps.get(1).map(|m| m.as_str());
            let py_name = caps.get(2).map(|m| m.as_str());
            match (fn_name, py_name) {
                (Some(name), _) if is_test_attribute(previous) || name.ends_with("_test") => {
                    out.push(name.to_owned());
                }
                (_, Some(name)) => out.push(name.to_owned()),
                _ => {}
            }
        }
        if !line.trim().is_empty() {
            previous = line;
        }
    }
    out
}

fn shortened(name: String) -> String {
    name.chars().take(NAME_CHARS).collect()
}

pub async fn test_names(repo: &str, sha: &str, files: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for file in files {
        let spec = format!("{sha}:{file}");
        let Some(raw) = git(repo, &["show", &spec], None).await else {
            continue;
        };
        let source = String::from_utf8_lossy(&raw);
        out.extend(
            names_in(&source)
                .into_iter()
                .map(|name| shortened(format!("{file}: {name}"))),
        );
        if out.len() >= MAX_NAMES {
            break;
        }
    }
    out.truncate(MAX_NAMES);
    out
}
