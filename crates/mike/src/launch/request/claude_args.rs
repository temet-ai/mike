use uuid::Uuid;

use super::super::error::LaunchError;

pub const SESSION_PLACEHOLDER: &str = "{{session_id}}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRule {
    Explicit(Uuid),
    /// `--resume <uuid>` without `--fork-session`: claude continues that
    /// session under its own id, so the launch knows it.
    Resumed(Uuid),
    Resume,
    Preassign,
}

pub fn session_rule(args: &[String]) -> Result<SessionRule, LaunchError> {
    if let Some(value) = read_value(args, "--session-id") {
        return value
            .to_lowercase()
            .parse::<Uuid>()
            .map(SessionRule::Explicit)
            .map_err(|_| LaunchError::Usage(format!("--session-id `{value}` is not a uuid")));
    }
    let resumes = args
        .iter()
        .any(|a| a == "--resume" || a == "--continue" || a.starts_with("--resume="));
    if !resumes {
        return Ok(SessionRule::Preassign);
    }
    let forks = args.iter().any(|a| a == "--fork-session");
    let continues = args.iter().any(|a| a == "--continue");
    let resumed = read_value(args, "--resume").and_then(|v| v.to_lowercase().parse::<Uuid>().ok());
    Ok(match resumed {
        Some(id) if !forks && !continues => SessionRule::Resumed(id),
        _ => SessionRule::Resume,
    })
}

pub fn read_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
        if let Some(value) = arg
            .strip_prefix(flag)
            .and_then(|rest| rest.strip_prefix('='))
        {
            return Some(value.to_string());
        }
    }
    None
}

pub fn has_remote_control(args: &[String]) -> bool {
    args.iter().any(|a| a.starts_with("--remote-control"))
}

pub fn has_channels(args: &[String]) -> bool {
    args.iter().any(|a| {
        a == super::super::DEV_CHANNELS_FLAG
            || a.starts_with("--dangerously-load-development-channels=")
    })
}

pub fn bare_prompt_present(args: &[String]) -> bool {
    let mut previous_was_flag = false;
    for arg in args {
        let is_flag = arg.starts_with('-');
        if !is_flag && !previous_was_flag {
            return true;
        }
        previous_was_flag = is_flag && !arg.contains('=');
    }
    false
}

pub fn substitute_session(text: &str, session: Uuid) -> String {
    text.replace(SESSION_PLACEHOLDER, &session.to_string())
}

pub fn slug(prompt: &str) -> String {
    let joined = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(32).collect()
}

pub fn title(model: Option<&str>, prompt: Option<&str>) -> String {
    let mut out = String::from("cc");
    if let Some(model) = model {
        out.push('·');
        out.push_str(model);
    }
    if let Some(prompt) = prompt.filter(|p| !p.trim().is_empty()) {
        out.push_str(" — ");
        out.push_str(&slug(prompt));
    }
    out
}
