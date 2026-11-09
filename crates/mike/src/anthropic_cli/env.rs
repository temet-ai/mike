use std::collections::HashMap;

use crate::environment::{Environment, ProcessEnvironment};

const ALLOWED_EXACT_VARS: &[&str] = &["PATH", "HOME", "USER", "LANG", "LC_ALL", "TMPDIR"];

pub(crate) fn clean_cli_env() -> HashMap<String, String> {
    clean_cli_env_from(&ProcessEnvironment)
}

pub(crate) fn clean_cli_env_from(environment: &dyn Environment) -> HashMap<String, String> {
    ALLOWED_EXACT_VARS
        .iter()
        .filter_map(|key| environment.var(key).map(|value| ((*key).to_owned(), value)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::MapEnvironment;

    #[test]
    fn clean_cli_env_never_carries_child_session_marker() {
        let environment = MapEnvironment::new(&[("CLAUDE_CODE_CHILD_SESSION", "1")]);
        let env = clean_cli_env_from(&environment);
        assert!(!env.contains_key("CLAUDE_CODE_CHILD_SESSION"));
    }

    #[test]
    fn clean_cli_env_never_carries_messaging_socket() {
        let environment = MapEnvironment::new(&[("CLAUDE_CODE_MESSAGING_SOCKET", "/tmp/socket")]);
        let env = clean_cli_env_from(&environment);
        assert!(!env.contains_key("CLAUDE_CODE_MESSAGING_SOCKET"));
    }

    #[test]
    fn clean_cli_env_never_carries_proxy_routing_variables() {
        let environment = MapEnvironment::new(&[
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:9988"),
            ("HTTPS_PROXY", "http://127.0.0.1:9987"),
        ]);
        assert!(clean_cli_env_from(&environment).is_empty());
    }

    #[test]
    fn clean_cli_env_carries_path_when_set() {
        let environment = MapEnvironment::new(&[("PATH", "/usr/bin:/bin")]);
        let env = clean_cli_env_from(&environment);
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
    }

    #[test]
    fn clean_cli_env_omits_allowed_variables_that_are_unset() {
        let environment = MapEnvironment::new(&[("PATH", "/bin")]);
        let env = clean_cli_env_from(&environment);
        assert_eq!(env.len(), 1);
    }
}
