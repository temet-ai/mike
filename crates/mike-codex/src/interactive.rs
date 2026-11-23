#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractiveArgs {
    pub args: Vec<String>,
    pub prompt: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InteractiveError {
    #[error(
        "`{0}` does not start a fresh local interactive Codex session; use Codex directly for this operation"
    )]
    NotFresh(String),
    #[error("`{0}` is a Claude option and cannot be passed to a Codex launch")]
    ClaudeOption(String),
    #[error("`{0}` needs a value")]
    MissingValue(String),
    #[error("Codex accepts one initial prompt; use --prompt-file for a complete brief")]
    MultiplePrompts,
}

pub fn parse_interactive(args: &[String]) -> Result<InteractiveArgs, InteractiveError> {
    let mut parsed = InteractiveArgs {
        args: Vec::new(),
        prompt: None,
        model: None,
    };
    let mut iter = args.iter();
    let mut positional = false;
    while let Some(arg) = iter.next() {
        if positional {
            set_prompt(&mut parsed, arg, false)?;
            continue;
        }
        if arg == "--" {
            positional = true;
            parsed.args.push(arg.clone());
            continue;
        }
        let (name, inline) = option_parts(arg);
        refuse_foreign_option(name)?;
        if takes_value(name) {
            let value = inline
                .or_else(|| iter.next().map(String::as_str))
                .filter(|value| !value.starts_with('-'))
                .ok_or_else(|| InteractiveError::MissingValue(name.into()))?;
            push_option(&mut parsed, arg, name, value, inline.is_some())?;
        } else if arg.starts_with('-') {
            parsed.args.push(arg.clone());
        } else {
            set_prompt(&mut parsed, arg, true)?;
        }
    }
    if !parsed.args.iter().any(|arg| arg == "--no-daemon") {
        parsed.args.insert(0, "--no-daemon".into());
    }
    Ok(parsed)
}

fn push_option(
    parsed: &mut InteractiveArgs,
    arg: &str,
    name: &str,
    value: &str,
    inline: bool,
) -> Result<(), InteractiveError> {
    if matches!(name, "--model" | "-m") {
        parsed.model = Some(value.into());
    }
    parsed.args.push(arg.into());
    if !inline {
        parsed.args.push(value.into());
    }
    Ok(())
}

pub fn proxy_args(base_url: &str) -> Vec<String> {
    vec![
        "-c".into(),
        format!(
            "openai_base_url={}",
            serde_json::Value::String(base_url.into())
        ),
    ]
}

fn option_parts(arg: &str) -> (&str, Option<&str>) {
    if let Some(short) = arg.get(..2)
        && matches!(short, "-c" | "-m" | "-p" | "-i" | "-s" | "-a" | "-C")
        && arg.len() > 2
    {
        return (
            short,
            arg.get(2..)
                .map(|value| value.strip_prefix('=').unwrap_or(value)),
        );
    }
    arg.split_once('=')
        .map_or((arg, None), |(key, value)| (key, Some(value)))
}

fn takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "-c" | "--config"
            | "--enable"
            | "--disable"
            | "-i"
            | "--image"
            | "-m"
            | "--model"
            | "--local-provider"
            | "-p"
            | "--profile"
            | "-s"
            | "--sandbox"
            | "--add-dir"
            | "-a"
            | "--ask-for-approval"
    )
}

fn refuse_foreign_option(arg: &str) -> Result<(), InteractiveError> {
    if matches!(
        arg,
        "--remote"
            | "--remote-auth-token-env"
            | "-C"
            | "--cd"
            | "--worktree"
            | "-h"
            | "--help"
            | "-V"
            | "--version"
    ) {
        return Err(InteractiveError::NotFresh(arg.into()));
    }
    if matches!(
        arg,
        "--session-id"
            | "--resume"
            | "--continue"
            | "--fork-session"
            | "--agent"
            | "--name"
            | "--remote-control"
            | "--mcp-config"
            | "--allowedTools"
            | "--dangerously-load-development-channels"
            | "--dangerously-skip-permissions"
    ) {
        return Err(InteractiveError::ClaudeOption(arg.into()));
    }
    Ok(())
}

fn set_prompt(
    parsed: &mut InteractiveArgs,
    prompt: &str,
    command: bool,
) -> Result<(), InteractiveError> {
    if command
        && matches!(
            prompt,
            "agents"
                | "exec"
                | "e"
                | "review"
                | "login"
                | "logout"
                | "mcp"
                | "plugin"
                | "app-server"
                | "remote-control"
                | "completion"
                | "update"
                | "doctor"
                | "sandbox"
                | "debug"
                | "apply"
                | "a"
                | "resume"
                | "queue"
                | "archive"
                | "delete"
                | "migrate-rollouts"
                | "migrate-nfs-db"
                | "unarchive"
                | "fork"
                | "cloud"
                | "exec-server"
                | "features"
                | "help"
        )
    {
        return Err(InteractiveError::NotFresh(prompt.into()));
    }
    if parsed.prompt.replace(prompt.into()).is_some() {
        return Err(InteractiveError::MultiplePrompts);
    }
    Ok(())
}

#[cfg(test)]
#[path = "interactive_tests.rs"]
mod tests;
