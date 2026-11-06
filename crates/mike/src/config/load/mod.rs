use figment::{
    Figment,
    providers::{Env, Serialized},
};

use crate::analysis::EmbeddingConfig;
use crate::config::ProxyConfig;

mod cli_overrides;
pub(crate) mod dotenv;
pub(crate) mod file;
mod file_normalize;
pub(crate) mod import;
mod redact;
mod shell_export;

pub use cli_overrides::CliOverrides;
pub(crate) use redact::redact_database_url;
pub(crate) use shell_export::{to_shell_exports, to_toml_string};

pub fn load(cli: CliOverrides) -> anyhow::Result<ProxyConfig> {
    load_layered(cli)
}

fn load_layered(cli: CliOverrides) -> anyhow::Result<ProxyConfig> {
    let paths = crate::config::paths::InstallationPaths::resolve(cli.config.as_deref())?;
    let snapshot =
        crate::config::child::read_snapshot(cli.boot_config_snapshot.as_deref(), &paths.state_dir)?;
    let table = if snapshot.is_some() {
        toml::Table::new()
    } else {
        file::read(&paths.config_file)?
    };
    file::validate(&table)?;
    let embedding_enabled = cli.embedding_enabled.or_else(|| {
        snapshot
            .as_ref()
            .and_then(|value| value.overrides.embedding_enabled)
    });
    let mut figment = Figment::from(Serialized::defaults(ProxyConfig::default()))
        .merge(Serialized::defaults(table));

    figment = figment.merge(env_provider());
    if let Some(raw) = std::env::var_os("MIKE_SPA_ORIGINS") {
        let parsed: Vec<String> = raw
            .to_string_lossy()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        figment = figment.merge(Serialized::default("spa_origins", parsed));
    }
    if let Some(raw) = std::env::var_os("MIKE_INSPECT_ALLOWED_HOSTS") {
        let parsed: Vec<String> = raw
            .to_string_lossy()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        figment = figment.merge(Serialized::default("inspect_allowed_hosts", parsed));
    }
    if let Some(raw) = std::env::var_os("MIKE_LAUNCH_CLAUDE_ARGS") {
        let parsed = crate::config::launch::parse_claude_args_env(&raw.to_string_lossy());
        figment = figment.merge(Serialized::default("launch_claude_args", parsed));
    }
    if let Some(snapshot) = &snapshot {
        figment = figment.merge(Serialized::defaults(&snapshot.config));
    }
    figment = figment.merge(Serialized::defaults(&cli));

    let mut config: ProxyConfig = figment.extract().map_err(|_| {
        anyhow::anyhow!(
            "failed to load boot configuration; check field types (values omitted for privacy)"
        )
    })?;

    for directory in [&mut config.data_dir, &mut config.log_dir]
        .into_iter()
        .flatten()
    {
        if directory.is_relative() {
            *directory = paths.config_dir.join(&*directory);
        }
    }
    config.config_path = paths.config_file;
    config.boot_overrides = snapshot.map_or(cli, |value| value.overrides);
    config.boot_overrides.config = Some(config.config_path.clone());
    config.data_dir.get_or_insert(paths.data_dir);
    config.log_dir.get_or_insert(paths.log_dir);
    apply_openrouter_env(&mut config);
    config.embedding = EmbeddingConfig::default();

    if let Some(v) = embedding_enabled {
        config.embedding.enabled = v;
    }

    Ok(config)
}

pub fn reload_selected(config: &ProxyConfig) -> anyhow::Result<ProxyConfig> {
    let mut cli = config.boot_overrides.clone();
    cli.config = Some(config.config_path.clone());
    cli.boot_config_snapshot = None;
    load(cli)
}

fn apply_openrouter_env(config: &mut ProxyConfig) {
    if let Ok(key) = std::env::var("MIKE_STEERER_OPENROUTER_API_KEY")
        .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
    {
        config.steerer.openrouter_api_key = Some(secrecy::SecretString::new(key.into()));
    }
}

fn env_provider() -> Env {
    Env::prefixed("MIKE_")
        .map(|key| {
            let lowered = key.as_str().to_ascii_lowercase();
            let mapped: &str = match lowered.as_str() {
                "listen_addr" => "proxy_listen_addr",
                "inspect_addr" => "inspect_listen_addr",
                _ => return key.into(),
            };
            mapped.into()
        })
        .split("__")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_env::*;

    #[test]
    fn defaults_apply_when_nothing_is_set() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let config = load_layered(CliOverrides::default())?;
        assert_eq!(config.proxy_listen_addr, "127.0.0.1:9988");
        assert_eq!(config.max_requests, 1000);
        assert!(config.persist_enabled, "persist_enabled defaults to true");
        Ok(())
    }

    #[test]
    fn env_layer_still_overrides_defaults() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g = EnvVarGuard::set("MIKE_LISTEN_ADDR", "127.0.0.1:7002");

        let config = load_layered(CliOverrides::default())?;
        assert_eq!(config.proxy_listen_addr, "127.0.0.1:7002");
        Ok(())
    }

    #[test]
    fn embedding_env_vars_are_ignored() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g_enabled = EnvVarGuard::set("MIKE_EMBEDDING__ENABLED", "1");
        let _g_url = EnvVarGuard::set("MIKE_EMBEDDING__URL", "http://127.0.0.1:11434/v1");
        let _g_model = EnvVarGuard::set("MIKE_EMBEDDING__MODEL", "ollama-model");
        let _g_chunk = EnvVarGuard::set("MIKE_EMBEDDING__CHUNK_MAX_CHARS", "64");

        let config = load_layered(CliOverrides::default())?;
        assert!(
            !config.embedding.enabled,
            "embedding env vars must be ignored"
        );
        assert_eq!(config.embedding.url, "http://127.0.0.1:7997");
        assert_eq!(config.embedding.model, "nomic-embed-text-v1.5");
        assert_eq!(config.embedding.chunk_max_chars, 1500);
        Ok(())
    }

    #[test]
    fn cli_layer_overrides_env() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g = EnvVarGuard::set("MIKE_LISTEN_ADDR", "127.0.0.1:7002");
        let cli = CliOverrides {
            proxy_listen_addr: Some("127.0.0.1:7003".to_string()),
            ..CliOverrides::default()
        };

        let config = load_layered(cli)?;
        assert_eq!(
            config.proxy_listen_addr, "127.0.0.1:7003",
            "CLI must win over env"
        );
        Ok(())
    }

    #[test]
    fn cli_embedding_override_wins() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let cli = CliOverrides {
            embedding_enabled: Some(false),
            ..CliOverrides::default()
        };

        let config = load_layered(cli)?;
        assert!(
            !config.embedding.enabled,
            "--embedding false must force runtime off"
        );
        Ok(())
    }

    #[test]
    fn every_documented_env_mapping_reaches_the_right_field() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g1 = EnvVarGuard::set("MIKE_LISTEN_ADDR", "127.0.0.1:7100");
        let _g2 = EnvVarGuard::set("MIKE_INSPECT_ADDR", "127.0.0.1:7101");
        let _g3 = EnvVarGuard::set("MIKE_UPSTREAM_URL", "https://example.test");
        let _g4 = EnvVarGuard::set("MIKE_MAX_REQUESTS", "42");
        let _g5 = EnvVarGuard::set("MIKE_MAX_AGE", "2h");
        let _g6 = EnvVarGuard::set("MIKE_PERSIST_ENABLED", "0");

        let config = load_layered(CliOverrides::default())?;
        assert_eq!(config.proxy_listen_addr, "127.0.0.1:7100");
        assert_eq!(config.inspect_listen_addr, "127.0.0.1:7101");
        assert_eq!(config.upstream_url, "https://example.test");
        assert_eq!(config.max_requests, 42);
        assert_eq!(config.max_age, "2h");
        assert!(!config.persist_enabled);
        Ok(())
    }

    #[test]
    fn db_fallback_and_resync_default_true() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let config = load_layered(CliOverrides::default())?;
        assert!(config.db_fallback_enabled);
        assert!(config.db_resync_enabled);
        Ok(())
    }

    #[test]
    fn local_database_url_env_lands_in_field() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g = EnvVarGuard::set(
            "MIKE_LOCAL_DATABASE_URL",
            "postgres://proxy@localhost:6666/claude_proxy_local",
        );
        let config = load_layered(CliOverrides::default())?;
        assert_eq!(
            config.local_database_url.as_deref(),
            Some("postgres://proxy@localhost:6666/claude_proxy_local")
        );
        Ok(())
    }

    #[test]
    fn db_fallback_env_can_disable() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g = EnvVarGuard::set("MIKE_DB_FALLBACK_ENABLED", "0");
        let config = load_layered(CliOverrides::default())?;
        assert!(!config.db_fallback_enabled);
        Ok(())
    }

    #[test]
    fn cli_db_flags_override_env() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g = EnvVarGuard::set("MIKE_DB_FALLBACK_ENABLED", "1");
        let cli = CliOverrides {
            db_fallback_enabled: Some(false),
            ..CliOverrides::default()
        };
        let config = load_layered(cli)?;
        assert!(!config.db_fallback_enabled, "CLI must win over env");
        Ok(())
    }

    #[test]
    fn smoke_run_defaults_false() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let config = load_layered(CliOverrides::default())?;
        assert!(
            !config.smoke_run,
            "smoke_run must default false — it is a structural safety guard, not opt-in behavior"
        );
        Ok(())
    }

    #[test]
    fn smoke_run_env_sets_it() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g = EnvVarGuard::set("MIKE_SMOKE_RUN", "1");
        let config = load_layered(CliOverrides::default())?;
        assert!(config.smoke_run, "MIKE_SMOKE_RUN=1 must enable smoke_run");
        Ok(())
    }

    #[test]
    fn smoke_run_cli_overrides_env() -> anyhow::Result<()> {
        let _serial = env_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("env_lock poisoned: {e}"))?;
        let _cleared = clear_proxy_env();

        let _g = EnvVarGuard::set("MIKE_SMOKE_RUN", "1");
        let cli = CliOverrides {
            smoke_run: Some(false),
            ..CliOverrides::default()
        };
        let config = load_layered(cli)?;
        assert!(
            !config.smoke_run,
            "CLI --smoke-run=false must win over MIKE_SMOKE_RUN=1"
        );
        Ok(())
    }
}

#[cfg(test)]
mod selected_tests;
