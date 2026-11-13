use crate::config::{CliOverrides, LaunchConfig, parse_permissive_bool_str};

const BACKEND: &str = "launch_backend";
const FOLLOW_CALLER: &str = "launch_backend_follow_caller";
const BACKEND_ENV: &str = "MIKE_LAUNCH_BACKEND";
const FOLLOW_CALLER_ENV: &str = "MIKE_LAUNCH_BACKEND_FOLLOW_CALLER";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PinnedLaunch {
    pub asked: Option<String>,
    backend: Option<String>,
    follow_caller: Option<bool>,
}

impl PinnedLaunch {
    pub fn from_sources(
        cli: &CliOverrides,
        env: impl Fn(&str) -> Option<String>,
        named_file: Option<&toml::Table>,
    ) -> Self {
        let named = |key: &str| named_file.and_then(|table| table.get(key));
        let asked = cli.launch_backend.clone().filter(|b| !b.trim().is_empty());
        let backend = asked
            .clone()
            .or_else(|| env(BACKEND_ENV))
            .or_else(|| {
                named(BACKEND)
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned)
            })
            .filter(|b| !b.trim().is_empty());
        let follow_caller = cli
            .launch_backend_follow_caller
            .or_else(|| env(FOLLOW_CALLER_ENV).and_then(|v| parse_permissive_bool_str(&v).ok()))
            .or_else(|| named(FOLLOW_CALLER).and_then(toml::Value::as_bool));
        Self {
            asked,
            backend,
            follow_caller,
        }
    }

    pub fn apply(&self, launch: &mut LaunchConfig) {
        if let Some(backend) = &self.backend {
            launch.backend.clone_from(backend);
        }
        if let Some(follow) = self.follow_caller {
            launch.follow_caller = follow;
        }
    }
}

pub fn named_file(cli: &CliOverrides) -> Option<toml::Table> {
    let path = cli.config.as_deref()?;
    crate::config::load::file::read(path).ok()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use super::*;

    fn stored_settings() -> LaunchConfig {
        LaunchConfig {
            backend: "herdr-core".into(),
            follow_caller: true,
            ..LaunchConfig::default()
        }
    }

    fn cli(backend: Option<&str>, follow: Option<bool>) -> CliOverrides {
        CliOverrides {
            launch_backend: backend.map(str::to_owned),
            launch_backend_follow_caller: follow,
            ..CliOverrides::default()
        }
    }

    #[test]
    fn command_line_flags_win_over_the_stored_settings_and_count_as_asked() {
        let pinned = PinnedLaunch::from_sources(&cli(Some("tmux"), Some(false)), |_| None, None);
        let mut launch = stored_settings();
        pinned.apply(&mut launch);
        assert_eq!(
            (launch.backend.as_str(), launch.follow_caller),
            ("tmux", false)
        );
        assert_eq!(pinned.asked.as_deref(), Some("tmux"));
    }

    #[test]
    fn a_named_config_file_wins_over_the_stored_settings_but_is_not_an_explicit_request() {
        let file: toml::Table =
            toml::from_str("launch_backend = \"tmux\"\nlaunch_backend_follow_caller = false\n")
                .unwrap();
        let pinned = PinnedLaunch::from_sources(&cli(None, None), |_| None, Some(&file));
        let mut launch = stored_settings();
        pinned.apply(&mut launch);
        assert_eq!(
            (launch.backend.as_str(), launch.follow_caller),
            ("tmux", false)
        );
        assert_eq!(pinned.asked, None);
    }

    #[test]
    fn the_flag_beats_the_environment_and_the_environment_beats_the_named_file() {
        let file: toml::Table = toml::from_str("launch_backend = \"herdr\"\n").unwrap();
        let env = |key: &str| (key == BACKEND_ENV).then(|| "none".to_string());
        let from_env = PinnedLaunch::from_sources(&cli(None, None), env, Some(&file));
        assert_eq!(from_env.backend.as_deref(), Some("none"));
        let from_flag = PinnedLaunch::from_sources(&cli(Some("tmux"), None), env, Some(&file));
        assert_eq!(from_flag.backend.as_deref(), Some("tmux"));
    }

    #[test]
    fn nothing_stated_for_this_command_leaves_the_stored_settings_alone() {
        let pinned = PinnedLaunch::from_sources(&cli(None, None), |_| None, None);
        let mut launch = stored_settings();
        pinned.apply(&mut launch);
        assert_eq!(launch, stored_settings());
    }
}
