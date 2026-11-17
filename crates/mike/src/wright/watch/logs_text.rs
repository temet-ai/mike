use chrono::{DateTime, Utc};

use super::logs::LogLine;

const TEXT_LEVELS: [&str; 5] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];
const COLLECTED_LEVELS: [&str; 2] = ["ERROR", "WARN"];
const PANIC_MARK: &str = "panicked at";
const PANIC_TARGET: &str = "panic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keep {
    Levels,
    PanicsOnly,
}

pub(crate) fn strip_ansi(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if !(next.is_ascii_digit() || next == ';') {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn is_target(segment: &str) -> bool {
    segment.split("::").all(|part| {
        let mut chars = part.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn split_target(rest: &str) -> (&str, &str) {
    let mut start = 0;
    for (index, _) in rest.match_indices(": ") {
        if index >= start && is_target(&rest[start..index]) {
            return (&rest[start..index], rest[index + 2..].trim_start());
        }
        start = index + 2;
    }
    ("", rest)
}

fn parse_timestamped(clean: &str) -> Option<LogLine> {
    let (stamp, rest) = clean.trim_start().split_once(char::is_whitespace)?;
    if !stamp.ends_with('Z') {
        return None;
    }
    let at: DateTime<Utc> = stamp.parse().ok()?;
    let rest = rest.trim_start();
    let (level, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    if !TEXT_LEVELS.contains(&level) {
        return None;
    }
    let (target, message) = split_target(rest.trim_start());
    Some(LogLine {
        at,
        level: level.to_owned(),
        target: target.to_owned(),
        message: message.to_owned(),
        raw: clean.to_owned(),
        origin: None,
    })
}

fn panic_line(clean: &str, at: DateTime<Utc>) -> LogLine {
    LogLine {
        at,
        level: "ERROR".to_owned(),
        target: PANIC_TARGET.to_owned(),
        message: clean.trim().to_owned(),
        raw: clean.to_owned(),
        origin: None,
    }
}

pub(crate) fn parse_text(text: &str, keep: Keep) -> Vec<LogLine> {
    let mut out = Vec::new();
    let mut previous: Option<DateTime<Utc>> = None;
    for raw in text.lines() {
        let clean = strip_ansi(raw);
        if let Some(line) = parse_timestamped(&clean) {
            previous = Some(line.at);
            if keep == Keep::Levels && COLLECTED_LEVELS.contains(&line.level.as_str()) {
                out.push(line);
            }
        } else if clean.contains(PANIC_MARK)
            && let Some(at) = previous
        {
            out.push(panic_line(&clean, at));
        }
    }
    out
}
