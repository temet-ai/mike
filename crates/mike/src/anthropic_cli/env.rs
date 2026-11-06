use std::collections::HashMap;

const ALLOWED_EXACT_VARS: &[&str] = &["PATH", "HOME", "USER", "LANG", "LC_ALL", "TMPDIR"];

pub(crate) fn clean_cli_env() -> HashMap<String, String> {
    let mut out = HashMap::new();
    for key in ALLOWED_EXACT_VARS {
        if let Ok(value) = std::env::var(key) {
            out.insert((*key).to_owned(), value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvVarGuard {
        key: &'static str,
        prior: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prior = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, prior }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.prior {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    #[serial_test::serial(anthropic_cli_env)]
    fn clean_cli_env_never_carries_child_session_marker() {
        let _guard = EnvVarGuard::set("CLAUDE_CODE_CHILD_SESSION", "1");
        let env = clean_cli_env();
        assert!(!env.contains_key("CLAUDE_CODE_CHILD_SESSION"));
    }

    #[test]
    #[serial_test::serial(anthropic_cli_env)]
    fn clean_cli_env_never_carries_messaging_socket() {
        let _guard = EnvVarGuard::set("CLAUDE_CODE_MESSAGING_SOCKET", "/tmp/socket");
        let env = clean_cli_env();
        assert!(!env.contains_key("CLAUDE_CODE_MESSAGING_SOCKET"));
    }

    #[test]
    #[serial_test::serial(anthropic_cli_env)]
    fn clean_cli_env_carries_path_when_set() {
        let _guard = EnvVarGuard::set("PATH", "/usr/bin:/bin");
        let env = clean_cli_env();
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
    }
}
