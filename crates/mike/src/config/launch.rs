use serde::{Deserialize, Serialize};

use super::bool_parse::deserialize_permissive_bool;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchConfig {
    #[serde(rename = "launch_backend")]
    pub(crate) backend: String,

    #[serde(
        rename = "launch_backend_follow_caller",
        deserialize_with = "deserialize_permissive_bool"
    )]
    pub(crate) follow_caller: bool,

    #[serde(
        rename = "launch_use_sidecar",
        deserialize_with = "deserialize_permissive_bool"
    )]
    pub(crate) use_sidecar: bool,

    #[serde(rename = "launch_proxy_forward_url")]
    pub(crate) proxy_forward_url: String,

    #[serde(rename = "launch_no_proxy")]
    pub(crate) no_proxy: String,

    #[serde(rename = "launch_ca_cert")]
    pub(crate) ca_cert: Option<String>,

    #[serde(rename = "launch_claude_args")]
    pub(crate) claude_args: Vec<String>,

    #[serde(rename = "launch_brief_inline_max")]
    pub(crate) brief_inline_max: usize,

    #[serde(
        rename = "launch_channel_check_enabled",
        deserialize_with = "deserialize_permissive_bool"
    )]
    pub(crate) channel_check_enabled: bool,

    #[serde(rename = "launch_channel_check_timeout_secs")]
    pub(crate) channel_check_timeout_secs: u64,

    #[serde(rename = "launch_channel_check_interval_secs")]
    pub(crate) channel_check_interval_secs: u64,

    #[serde(rename = "launch_warn_surfaces")]
    pub(crate) warn_surfaces: u32,

    #[serde(rename = "launch_max_surfaces")]
    pub(crate) max_surfaces: u32,

    #[serde(
        rename = "launch_auto_trust",
        deserialize_with = "deserialize_permissive_bool"
    )]
    pub(crate) auto_trust: bool,

    #[serde(rename = "launch_kill_wait_secs")]
    pub(crate) kill_wait_secs: u64,

    #[serde(rename = "launch_worktree_config")]
    pub(crate) worktree_config: Option<String>,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            backend: "none".to_string(),
            follow_caller: true,
            use_sidecar: false,
            proxy_forward_url: "http://127.0.0.1:9987".to_string(),
            no_proxy: "localhost,127.0.0.1".to_string(),
            ca_cert: None,
            claude_args: Vec::new(),
            brief_inline_max: DEFAULT_BRIEF_INLINE_MAX,
            channel_check_enabled: true,
            channel_check_timeout_secs: 90,
            channel_check_interval_secs: 3,
            warn_surfaces: 8,
            max_surfaces: 16,
            auto_trust: false,
            kill_wait_secs: 20,
            worktree_config: None,
        }
    }
}

#[cfg(any(test, feature = "test-seams"))]
impl LaunchConfig {
    pub(crate) fn trusting() -> Self {
        Self {
            auto_trust: true,
            ..Self::default()
        }
    }
}

pub(crate) fn parse_claude_args_env(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_string).collect()
}

/// Default size, in bytes, up to which a launch brief is inlined (launch::brief).
pub const DEFAULT_BRIEF_INLINE_MAX: usize = 800;

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::config::ProxyConfig;
    use crate::config::load::CliOverrides;
    use crate::environment::MapEnvironment;

    fn parse(args: &[&str], environment: &MapEnvironment) -> anyhow::Result<ProxyConfig> {
        let overrides = CliOverrides::try_parse_from(args)?;
        crate::config::load::load_with(overrides, environment)
    }

    #[test]
    fn launch_defaults_match_the_contract() {
        let cfg = LaunchConfig::default();
        assert_eq!(
            (cfg.backend.as_str(), cfg.use_sidecar, cfg.auto_trust),
            ("none", false, false)
        );
        assert_eq!(cfg.proxy_forward_url, "http://127.0.0.1:9987");
        assert_eq!(cfg.no_proxy, "localhost,127.0.0.1");
        assert_eq!(cfg.ca_cert, None);
        assert!(cfg.claude_args.is_empty());
        assert_eq!(cfg.brief_inline_max, 800);
        assert!(cfg.channel_check_enabled);
        assert_eq!(cfg.channel_check_timeout_secs, 90);
        assert_eq!(cfg.channel_check_interval_secs, 3);
        assert_eq!(cfg.warn_surfaces, 8);
        assert_eq!(cfg.max_surfaces, 16);
        assert_eq!(cfg.kill_wait_secs, 20);
        assert_eq!(cfg.worktree_config, None);
    }

    #[test]
    fn launch_env_vars_reach_every_field() -> anyhow::Result<()> {
        let environment = MapEnvironment::isolated()
            .with("MIKE_LAUNCH_BACKEND", "herdr")
            .with("MIKE_LAUNCH_BACKEND_FOLLOW_CALLER", "0")
            .with("MIKE_LAUNCH_USE_SIDECAR", "1")
            .with("MIKE_LAUNCH_PROXY_FORWARD_URL", "http://127.0.0.1:19987")
            .with("MIKE_LAUNCH_NO_PROXY", "localhost")
            .with("MIKE_LAUNCH_CA_CERT", "/tmp/ca.pem")
            .with(
                "MIKE_LAUNCH_CLAUDE_ARGS",
                "--dangerously-skip-permissions  --chrome",
            )
            .with("MIKE_LAUNCH_BRIEF_INLINE_MAX", "1200")
            .with("MIKE_LAUNCH_CHANNEL_CHECK_ENABLED", "0")
            .with("MIKE_LAUNCH_CHANNEL_CHECK_TIMEOUT_SECS", "45")
            .with("MIKE_LAUNCH_CHANNEL_CHECK_INTERVAL_SECS", "5")
            .with("MIKE_LAUNCH_WARN_SURFACES", "4")
            .with("MIKE_LAUNCH_MAX_SURFACES", "0")
            .with("MIKE_LAUNCH_AUTO_TRUST", "no")
            .with("MIKE_LAUNCH_KILL_WAIT_SECS", "7")
            .with("MIKE_LAUNCH_WORKTREE_CONFIG", "a.toml:b.json");
        let cfg = parse(&["mike"], &environment)?.launch;
        assert_eq!(
            cfg,
            LaunchConfig {
                backend: "herdr".into(),
                follow_caller: false,
                use_sidecar: true,
                proxy_forward_url: "http://127.0.0.1:19987".into(),
                no_proxy: "localhost".into(),
                ca_cert: Some("/tmp/ca.pem".into()),
                claude_args: vec!["--dangerously-skip-permissions".into(), "--chrome".into()],
                brief_inline_max: 1200,
                channel_check_enabled: false,
                channel_check_timeout_secs: 45,
                channel_check_interval_secs: 5,
                warn_surfaces: 4,
                max_surfaces: 0,
                auto_trust: false,
                kill_wait_secs: 7,
                worktree_config: Some("a.toml:b.json".into()),
            }
        );
        Ok(())
    }

    #[test]
    fn launch_cli_flags_override_env() -> anyhow::Result<()> {
        let environment = MapEnvironment::isolated()
            .with("MIKE_LAUNCH_BACKEND", "tmux")
            .with("MIKE_LAUNCH_CLAUDE_ARGS", "--chrome");
        let cfg = parse(
            &[
                "mike",
                "--launch-backend",
                "herdr",
                "--launch-claude-args",
                "--dangerously-skip-permissions --verbose",
                "--launch-auto-trust",
                "0",
                "--launch-kill-wait-secs",
                "3",
                "--launch-ca-cert",
                "/x/ca.pem",
                "--launch-worktree-config",
                "c.toml:d.json",
            ],
            &environment,
        )?
        .launch;
        assert_eq!(cfg.backend, "herdr");
        assert_eq!(
            cfg.claude_args,
            vec!["--dangerously-skip-permissions", "--verbose"]
        );
        assert!(!cfg.auto_trust);
        assert_eq!(cfg.kill_wait_secs, 3);
        assert_eq!(cfg.ca_cert.as_deref(), Some("/x/ca.pem"));
        assert_eq!(cfg.worktree_config.as_deref(), Some("c.toml:d.json"));
        Ok(())
    }

    #[test]
    fn launch_toml_round_trips_through_flat_keys() -> anyhow::Result<()> {
        let mut cfg = ProxyConfig::for_tests_in_memory();
        cfg.launch.backend = "herdr".into();
        cfg.launch.claude_args = vec!["--chrome".into()];
        let toml = crate::config::load::to_toml_string(&cfg)?;
        assert!(toml.contains("launch_backend = \"herdr\""), "{toml}");
        assert!(
            toml.contains("launch_claude_args = [\"--chrome\"]"),
            "{toml}"
        );
        let back: ProxyConfig = toml::from_str(&toml)?;
        assert_eq!(back.launch, cfg.launch);
        Ok(())
    }

    #[test]
    fn parse_claude_args_env_splits_on_any_whitespace_and_drops_empties() {
        assert_eq!(
            parse_claude_args_env("  --a\t--b  \n--c "),
            vec!["--a", "--b", "--c"]
        );
        assert!(parse_claude_args_env("   ").is_empty());
    }

    #[test]
    fn launch_bad_bool_env_is_a_load_error() -> anyhow::Result<()> {
        let environment = MapEnvironment::isolated().with("MIKE_LAUNCH_AUTO_TRUST", "maybe");
        let err = parse(&["mike"], &environment).err();
        assert!(
            err.is_some(),
            "a non-bool MIKE_LAUNCH_AUTO_TRUST must fail the load"
        );
        Ok(())
    }
}
