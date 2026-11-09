use super::*;
use crate::environment::MapEnvironment;

#[test]
fn defaults_apply_when_nothing_is_set() -> anyhow::Result<()> {
    let config = load_layered(CliOverrides::default(), &MapEnvironment::isolated())?;
    assert_eq!(config.proxy_listen_addr, "127.0.0.1:9988");
    assert_eq!(config.max_requests, 1000);
    assert!(config.persist_enabled, "persist_enabled defaults to true");
    Ok(())
}

#[test]
fn env_layer_still_overrides_defaults() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated().with("MIKE_LISTEN_ADDR", "127.0.0.1:7002");

    let config = load_layered(CliOverrides::default(), &environment)?;
    assert_eq!(config.proxy_listen_addr, "127.0.0.1:7002");
    Ok(())
}

#[test]
fn embedding_env_vars_are_ignored() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated()
        .with("MIKE_EMBEDDING__ENABLED", "1")
        .with("MIKE_EMBEDDING__URL", "http://127.0.0.1:11434/v1")
        .with("MIKE_EMBEDDING__MODEL", "ollama-model")
        .with("MIKE_EMBEDDING__CHUNK_MAX_CHARS", "64");

    let config = load_layered(CliOverrides::default(), &environment)?;
    assert!(
        !config.embedding.enabled,
        "embedding env vars must be ignored"
    );
    assert_eq!(config.embedding.url, "http://127.0.0.1:7997");
    assert_eq!(config.embedding.model, "embeddinggemma");
    assert_eq!(config.embedding.chunk_max_chars, 1500);
    Ok(())
}

#[test]
fn cli_layer_overrides_env() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated().with("MIKE_LISTEN_ADDR", "127.0.0.1:7002");
    let cli = CliOverrides {
        proxy_listen_addr: Some("127.0.0.1:7003".to_string()),
        ..CliOverrides::default()
    };

    let config = load_layered(cli, &environment)?;
    assert_eq!(
        config.proxy_listen_addr, "127.0.0.1:7003",
        "CLI must win over env"
    );
    Ok(())
}

#[test]
fn cli_embedding_override_wins() -> anyhow::Result<()> {
    let cli = CliOverrides {
        embedding_enabled: Some(false),
        ..CliOverrides::default()
    };

    let config = load_layered(cli, &MapEnvironment::isolated())?;
    assert!(
        !config.embedding.enabled,
        "--embedding false must force runtime off"
    );
    Ok(())
}

#[test]
fn every_documented_env_mapping_reaches_the_right_field() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated()
        .with("MIKE_LISTEN_ADDR", "127.0.0.1:7100")
        .with("MIKE_INSPECT_ADDR", "127.0.0.1:7101")
        .with("MIKE_UPSTREAM_URL", "https://example.test")
        .with("MIKE_MAX_REQUESTS", "42")
        .with("MIKE_MAX_AGE", "2h")
        .with("MIKE_PERSIST_ENABLED", "0");

    let config = load_layered(CliOverrides::default(), &environment)?;
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
    let config = load_layered(CliOverrides::default(), &MapEnvironment::isolated())?;
    assert!(config.db_fallback_enabled);
    assert!(config.db_resync_enabled);
    Ok(())
}

#[test]
fn local_database_url_env_lands_in_field() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated().with(
        "MIKE_LOCAL_DATABASE_URL",
        "postgres://proxy@localhost:6666/claude_proxy_local",
    );
    let config = load_layered(CliOverrides::default(), &environment)?;
    assert_eq!(
        config.local_database_url.as_deref(),
        Some("postgres://proxy@localhost:6666/claude_proxy_local")
    );
    Ok(())
}

#[test]
fn db_fallback_env_can_disable() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated().with("MIKE_DB_FALLBACK_ENABLED", "0");
    let config = load_layered(CliOverrides::default(), &environment)?;
    assert!(!config.db_fallback_enabled);
    Ok(())
}

#[test]
fn cli_db_flags_override_env() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated().with("MIKE_DB_FALLBACK_ENABLED", "1");
    let cli = CliOverrides {
        db_fallback_enabled: Some(false),
        ..CliOverrides::default()
    };
    let config = load_layered(cli, &environment)?;
    assert!(!config.db_fallback_enabled, "CLI must win over env");
    Ok(())
}

#[test]
fn smoke_run_defaults_false() -> anyhow::Result<()> {
    let config = load_layered(CliOverrides::default(), &MapEnvironment::isolated())?;
    assert!(
        !config.smoke_run,
        "smoke_run must default false — it is a structural safety guard, not opt-in behavior"
    );
    Ok(())
}

#[test]
fn smoke_run_env_sets_it() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated().with("MIKE_SMOKE_RUN", "1");
    let config = load_layered(CliOverrides::default(), &environment)?;
    assert!(config.smoke_run, "MIKE_SMOKE_RUN=1 must enable smoke_run");
    Ok(())
}

#[test]
fn smoke_run_cli_overrides_env() -> anyhow::Result<()> {
    let environment = MapEnvironment::isolated().with("MIKE_SMOKE_RUN", "1");
    let cli = CliOverrides {
        smoke_run: Some(false),
        ..CliOverrides::default()
    };
    let config = load_layered(cli, &environment)?;
    assert!(
        !config.smoke_run,
        "CLI --smoke-run=false must win over MIKE_SMOKE_RUN=1"
    );
    Ok(())
}

#[test]
fn spa_origins_env_is_split_on_commas_and_trimmed() -> anyhow::Result<()> {
    let environment =
        MapEnvironment::isolated().with("MIKE_SPA_ORIGINS", " http://a.test , ,http://b.test");
    let config = load_layered(CliOverrides::default(), &environment)?;
    assert_eq!(config.spa_origins, vec!["http://a.test", "http://b.test"]);
    Ok(())
}

#[test]
fn steerer_key_env_prefers_the_prefixed_name_over_the_plain_one() -> anyhow::Result<()> {
    use secrecy::ExposeSecret;
    let environment = MapEnvironment::isolated()
        .with("MIKE_STEERER_OPENROUTER_API_KEY", "prefixed")
        .with("OPENROUTER_API_KEY", "plain");
    let config = load_layered(CliOverrides::default(), &environment)?;
    assert_eq!(
        config
            .steerer
            .openrouter_api_key
            .as_ref()
            .map(|key| key.expose_secret().to_owned()),
        Some("prefixed".to_owned())
    );
    Ok(())
}

#[test]
fn config_home_env_selects_the_config_file_that_is_read() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config_dir = home.path().join("mike");
    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(config_dir.join("config.toml"), "max_requests = 77\n")?;
    let environment = MapEnvironment::isolated().with(
        crate::config::paths::CONFIG_HOME_ENV,
        &home.path().to_string_lossy(),
    );

    let config = load_layered(CliOverrides::default(), &environment)?;
    assert_eq!(config.max_requests, 77);
    Ok(())
}
