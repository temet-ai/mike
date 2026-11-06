use std::path::{Component, Path};

pub(super) fn work(command: &str) -> Option<String> {
    let words = shell_words(command)?;
    let (at, _) = build_program(&words)?;
    Some(words[at..].join(" "))
}

pub(super) fn directory(command: &str) -> Option<String> {
    let words = shell_words(command)?;
    let (at, environment) = build_program(&words)?;
    let arguments = &words[at + 1..];
    if arguments.iter().any(|word| {
        word.starts_with("-C")
            || word.starts_with("--directory")
            || word.starts_with("--manifest-path")
            || word.starts_with("--config")
    }) {
        return None;
    }
    let target = target_argument(arguments)?.or(environment);
    normalize(target.as_deref().unwrap_or("target"))
}

fn build_program(words: &[String]) -> Option<(usize, Option<String>)> {
    let mut at = 0;
    let mut target = None;
    while let Some(word) = words.get(at) {
        if let Some((name, value)) = word.split_once('=') {
            if name == "CARGO_TARGET_DIR" {
                target = Some(value.to_string());
            }
            at += 1;
            continue;
        }
        let program = word.rsplit('/').next()?;
        if matches!(
            program,
            "cargo" | "cx" | "make" | "gate" | "lint-rust" | "sqlx-prepare"
        ) {
            return Some((at, target));
        }
        at = after_wrapper(words, at, program)?;
    }
    None
}

fn after_wrapper(words: &[String], at: usize, program: &str) -> Option<usize> {
    match program {
        "env" | "command" | "exec" | "nohup" => Some(at + 1),
        "time" => Some(at + 1 + usize::from(words.get(at + 1).is_some_and(|w| w == "-p"))),
        "timeout" => {
            words.get(at + 1).filter(|word| !word.starts_with('-'))?;
            Some(at + 2)
        }
        "nice" if words.get(at + 1).is_some_and(|w| w == "-n") => {
            words.get(at + 2)?.parse::<i32>().ok()?;
            Some(at + 3)
        }
        "nice" => Some(at + 1),
        _ => None,
    }
}

fn target_argument(words: &[String]) -> Option<Option<String>> {
    let mut target = None;
    let mut words = words.iter();
    while let Some(word) = words.next() {
        if word == "--" {
            break;
        }
        if word == "--target-dir" {
            target = Some(words.next()?.clone());
        } else if let Some(value) = word.strip_prefix("--target-dir=") {
            target = Some(value.to_string());
        } else if let Some(value) = word.strip_prefix("CARGO_TARGET_DIR=") {
            target = Some(value.to_string());
        }
    }
    Some(target)
}

fn normalize(directory: &str) -> Option<String> {
    if directory.is_empty() || directory.starts_with(['~', '-']) {
        return None;
    }
    let mut normalized = std::path::PathBuf::new();
    for part in Path::new(directory).components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => return None,
            other => normalized.push(other.as_os_str()),
        }
    }
    Some(normalized.to_string_lossy().into_owned())
}

fn shell_words(command: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    for ch in command.chars() {
        if matches!(ch, '$' | '`' | '\\' | '\n') {
            return None;
        }
        match quote {
            Some(delimiter) if ch == delimiter => quote = None,
            Some(_) => word.push(ch),
            None if matches!(ch, '\'' | '"') => quote = Some(ch),
            None if matches!(
                ch,
                ';' | '&' | '|' | '<' | '>' | '(' | ')' | '{' | '}' | '#' | '*' | '?' | '['
            ) =>
            {
                return None;
            }
            None if ch.is_whitespace() => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            None => word.push(ch),
        }
    }
    if quote.is_some() {
        return None;
    }
    if !word.is_empty() {
        words.push(word);
    }
    Some(words)
}

#[cfg(test)]
#[path = "build_target_tests.rs"]
mod tests;
