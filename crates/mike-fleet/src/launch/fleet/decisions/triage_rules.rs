use std::sync::OnceLock;

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Retry,
    FixCode,
    FixEnvironment,
}

impl Action {
    pub fn key(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::FixCode => "fix_code",
            Self::FixEnvironment => "fix_env",
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Self::Retry => "retry, it looks transient",
            Self::FixCode => "fix the code",
            Self::FixEnvironment => "fix the environment, the code is probably fine",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "retry" => Some(Self::Retry),
            "fix_code" => Some(Self::FixCode),
            "fix_env" => Some(Self::FixEnvironment),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Triage {
    pub action: Action,
    pub reason: String,
    pub by: &'static str,
}

impl Triage {
    pub fn line(&self) -> String {
        format!(
            "triage ({}): {}; {}",
            self.by,
            self.action.words(),
            self.reason
        )
    }
}

const KILLED_EXITS: [i32; 3] = [124, 137, 143];

fn pattern(cell: &'static OnceLock<Option<Regex>>, source: &str) -> Option<&'static Regex> {
    cell.get_or_init(|| Regex::new(source).ok()).as_ref()
}

fn missing_module(output: &str) -> Option<String> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let re = pattern(
        &RE,
        r#"(?m)(ModuleNotFoundError: No module named '([^']+)'|Cannot find module '([^']+)'|error\[E0463\]: can't find crate for `([^`]+)`|(\S+): command not found|linker `([^`]+)` not found)"#,
    )?;
    let found = re.captures(output)?;
    let name = (2..=6).find_map(|i| found.get(i)).map(|m| m.as_str())?;
    Some(format!("a missing module, crate or command `{name}`"))
}

fn timed_out(exit: Option<i32>, output: &str) -> Option<String> {
    if exit.is_none() {
        return Some("the command was killed or never exited".to_owned());
    }
    if exit.is_some_and(|code| KILLED_EXITS.contains(&code)) {
        return Some(format!(
            "the command ended with exit {}, a timeout or a kill",
            exit.unwrap_or_default()
        ));
    }
    let lowered = output.to_lowercase();
    [
        "timed out",
        "timeout expired",
        "terminated by signal",
        "reaper",
        "killed\n",
    ]
    .iter()
    .find(|marker| lowered.contains(*marker))
    .map(|marker| format!("the output says `{}`", marker.trim()))
}

fn compile_error(output: &str) -> Option<String> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let re = pattern(
        &RE,
        r"(?m)(error(?:\[E\d+\])?:[^\n]*\n\s*--> ([^\s:]+:\d+)|^([^\s(]+)\((\d+),\d+\): error TS\d+|File \x22([^\x22]+)\x22, line (\d+)[^\n]*\n(?:[^\n]*\n)*?\s*SyntaxError)",
    )?;
    let found = re.captures(output)?;
    if let Some(at) = found.get(2) {
        return Some(format!("a compile error at {}", at.as_str()));
    }
    if let (Some(file), Some(line)) = (found.get(3), found.get(4)) {
        return Some(format!(
            "a type error at {}:{}",
            file.as_str(),
            line.as_str()
        ));
    }
    let (file, line) = (found.get(5)?, found.get(6)?);
    Some(format!(
        "a syntax error at {}:{}",
        file.as_str(),
        line.as_str()
    ))
}

pub fn by_rules(exit: Option<i32>, output: &str) -> Option<Triage> {
    let rule = |action, reason| Triage {
        action,
        reason,
        by: "rules",
    };
    if let Some(reason) = timed_out(exit, output) {
        return Some(rule(Action::FixEnvironment, reason));
    }
    if let Some(reason) = missing_module(output) {
        return Some(rule(Action::FixEnvironment, reason));
    }
    compile_error(output).map(|reason| rule(Action::FixCode, reason))
}

#[cfg(test)]
#[path = "triage_rules_tests.rs"]
mod tests;
