use std::collections::BTreeMap;

use crate::launch::error::LaunchError;

pub const CARRIED: [&str; 14] = [
    "HOME",
    "USER",
    "LOGNAME",
    "PATH",
    "SHELL",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "SSH_AUTH_SOCK",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_STATE_HOME",
    "XDG_DATA_HOME",
];

const PANE_TERM: &str = "\"TERM=$TERM\"";

pub fn seat_env(
    caller: impl Fn(&str) -> Option<String>,
    launch: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = CARRIED
        .iter()
        .filter_map(|key| Some((key.to_string(), caller(key).filter(|v| !v.is_empty())?)))
        .collect();
    env.extend(launch.iter().map(|(k, v)| (k.clone(), v.clone())));
    env
}

pub fn command_line(
    program: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<String, LaunchError> {
    let mut command = vec![quote(program)];
    command.extend(args.iter().map(|a| quote(a)));
    let command = command.join(" ");
    let mut line = vec!["env".to_string(), "-i".to_string(), PANE_TERM.to_string()];
    line.extend(env.iter().map(|(k, v)| quote(&format!("{k}={v}"))));
    match crate::launch::fleet::seat_helpers::setup(args).map_err(LaunchError::Failed)? {
        None => line.push(command),
        Some(setup) => {
            line.push("sh".to_string());
            line.push("-c".to_string());
            line.push(quote(&format!("{} && exec {command}", setup.command)));
        }
    }
    Ok(line.join(" "))
}

pub fn quote(arg: &str) -> String {
    let safe = arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:=,@".contains(c));
    if safe && !arg.is_empty() {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

#[cfg(any(test, feature = "test-seams"))]
#[allow(
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use super::*;

    fn operator_shell(key: &str) -> Option<String> {
        match key {
            "HOME" => Some("/home/op".into()),
            "PATH" => Some("/usr/local/bin:/usr/bin".into()),
            "LANG" => Some(String::new()),
            "CLAUDE_API_KEY" => Some("sk-operator".into()),
            "CLAUDE_CODE_PLUGIN_DIRS" => Some("/home/op/plugins".into()),
            "ANTHROPIC_API_KEY" => Some("sk-ant".into()),
            _ => None,
        }
    }

    #[test]
    fn a_seat_gets_the_carried_names_and_its_launch_values_but_none_of_the_operators_keys() {
        let mut launch = BTreeMap::new();
        launch.insert("MIKE_SESSION_ID".to_string(), "abc".to_string());
        launch.insert("PATH".to_string(), "/seat/bin".to_string());
        let env = seat_env(operator_shell, &launch);
        assert_eq!(env.get("HOME").map(String::as_str), Some("/home/op"));
        assert_eq!(env.get("PATH").map(String::as_str), Some("/seat/bin"));
        assert_eq!(env.get("MIKE_SESSION_ID").map(String::as_str), Some("abc"));
        assert!(!env.contains_key("LANG"), "an empty value is not carried");
        for leaked in [
            "CLAUDE_API_KEY",
            "CLAUDE_CODE_PLUGIN_DIRS",
            "ANTHROPIC_API_KEY",
        ] {
            assert!(
                !env.contains_key(leaked),
                "{leaked} reached the seat: {env:?}"
            );
        }
    }

    #[test]
    fn the_typed_line_clears_the_shell_environment_and_keeps_the_panes_terminal_type() {
        let mut env = BTreeMap::new();
        env.insert("HOME".to_string(), "/home/op".to_string());
        env.insert("NOTE".to_string(), "two words".to_string());
        let line = command_line("claude", &["--model".into(), "opus".into()], &env).unwrap();
        assert_eq!(
            line,
            "env -i \"TERM=$TERM\" HOME=/home/op 'NOTE=two words' claude --model opus"
        );
    }

    #[test]
    fn quote_passes_bare_tokens_and_single_quotes_the_rest() {
        assert_eq!(quote("--model"), "--model");
        assert_eq!(quote("/tmp/a.md"), "/tmp/a.md");
        assert_eq!(quote("hello world"), "'hello world'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote(""), "''");
    }
}
