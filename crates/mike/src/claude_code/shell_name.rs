use std::ffi::OsStr;

use super::error::ClaudeCodeError;
use crate::launch::cli::locate_executable;

pub(crate) const DEFAULT_NAME: &str = "cc";
const FALLBACK_NAMES: [&str; 3] = ["mcc", "cmike", "claude-mike"];
const MAX_LEN: usize = 32;

const RESERVED: [&str; 38] = [
    "alias", "bg", "bind", "break", "builtin", "case", "cd", "command", "continue", "declare",
    "do", "done", "echo", "elif", "else", "esac", "eval", "exec", "exit", "export", "fg", "fi",
    "for", "function", "if", "in", "jobs", "local", "read", "return", "set", "source", "test",
    "then", "type", "unset", "while", "wait",
];

fn reason(name: &str, path_var: Option<&OsStr>) -> Option<String> {
    if name.is_empty() {
        return Some("it is empty".to_string());
    }
    if name.len() > MAX_LEN {
        return Some(format!("it is longer than {MAX_LEN} characters"));
    }
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Some(
            "it must start with a letter or underscore and use only letters, digits, underscores and hyphens"
                .to_string(),
        );
    }
    if RESERVED.contains(&name) {
        return Some("the shell already uses it as a word or builtin".to_string());
    }
    locate_executable(name, path_var).map(|found| {
        format!(
            "it is already a command on PATH ({}), and a function of that name would hide it",
            found.display()
        )
    })
}

pub(crate) fn validate(name: &str, path_var: Option<&OsStr>) -> Result<(), ClaudeCodeError> {
    match reason(name, path_var) {
        None => Ok(()),
        Some(reason) => Err(ClaudeCodeError::BadFunctionName {
            name: name.to_string(),
            reason,
        }),
    }
}

pub(crate) fn suggest(path_var: Option<&OsStr>) -> String {
    std::iter::once(DEFAULT_NAME)
        .chain(FALLBACK_NAMES)
        .find(|name| reason(name, path_var).is_none())
        .unwrap_or(DEFAULT_NAME)
        .to_string()
}
