use super::*;

fn lookup_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| {
        pairs
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| (*value).to_string())
    }
}

fn config_from(pairs: &[(&str, &str)], overrides: ConfigOverrides) -> anyhow::Result<Config> {
    Config::from_lookup(overrides, &lookup_of(pairs)).map_err(|e| anyhow::anyhow!(e))
}

fn config_error(pairs: &[(&str, &str)]) -> anyhow::Result<String> {
    match Config::from_lookup(ConfigOverrides::default(), &lookup_of(pairs)) {
        Err(error) => Ok(error),
        Ok(_) => anyhow::bail!("expected the configuration to be rejected"),
    }
}

#[test]
fn defaults_apply_when_env_and_overrides_absent() -> anyhow::Result<()> {
    let cfg = config_from(&[], ConfigOverrides::default())?;
    assert_eq!(cfg.bind, "127.0.0.1:9987");
    assert_eq!(cfg.upstream, "http://127.0.0.1:9988");
    assert_eq!(cfg.intercept_host, "api.anthropic.com");
    assert_eq!(cfg.health_url, "http://127.0.0.1:9989/api/health");
    assert_eq!(cfg.probe_interval, Duration::from_secs(2));
    assert_eq!(cfg.probe_timeout, Duration::from_secs(1));
    assert_eq!(cfg.probe_mode, ProbeMode::Http);
    Ok(())
}

#[test]
fn env_overrides_defaults() -> anyhow::Result<()> {
    let cfg = config_from(
        &[
            ("MIKE_FORWARD_BIND", "0.0.0.0:1234"),
            (
                "MIKE_FORWARD_HEALTH_URL",
                "http://localhost:7777/api/health",
            ),
            ("MIKE_FORWARD_PROBE_INTERVAL", "5s"),
        ],
        ConfigOverrides::default(),
    )?;
    assert_eq!(cfg.bind, "0.0.0.0:1234");
    assert_eq!(cfg.health_url, "http://localhost:7777/api/health");
    assert_eq!(cfg.probe_interval, Duration::from_secs(5));
    Ok(())
}

#[test]
fn flag_override_beats_env() -> anyhow::Result<()> {
    let overrides = ConfigOverrides {
        bind: Some("127.0.0.1:5555".to_string()),
        ..Default::default()
    };
    let cfg = config_from(&[("MIKE_FORWARD_BIND", "0.0.0.0:1234")], overrides)?;
    assert_eq!(cfg.bind, "127.0.0.1:5555");
    Ok(())
}

#[test]
fn blank_env_value_is_treated_as_unset() -> anyhow::Result<()> {
    let cfg = config_from(&[("MIKE_FORWARD_BIND", "   ")], ConfigOverrides::default())?;
    assert_eq!(cfg.bind, DEFAULT_BIND);
    Ok(())
}

#[test]
fn bad_duration_is_rejected() -> anyhow::Result<()> {
    let err = config_error(&[("MIKE_FORWARD_PROBE_INTERVAL", "not-a-duration")])?;
    assert!(err.contains("MIKE_FORWARD_PROBE_INTERVAL"));
    assert!(err.contains("not a valid duration"));
    Ok(())
}

#[test]
fn unknown_probe_mode_is_rejected() -> anyhow::Result<()> {
    let err = config_error(&[("MIKE_FORWARD_PROBE_MODE", "ping")])?;
    assert!(err.contains("unknown probe mode"));
    Ok(())
}

#[test]
fn probe_mode_parses_http_aliases() {
    assert_eq!(ProbeMode::parse("http"), Ok(ProbeMode::Http));
    assert_eq!(ProbeMode::parse(" http_health "), Ok(ProbeMode::Http));
    assert!(ProbeMode::parse("tcp").is_err());
}

#[test]
fn render_lists_every_field() -> anyhow::Result<()> {
    let cfg = Config {
        bind: "127.0.0.1:9987".into(),
        upstream: "http://127.0.0.1:9988".into(),
        upstream_authority: "127.0.0.1:9988"
            .parse()
            .map_err(|e| anyhow::anyhow!("{e}"))?,
        intercept_host: "api.anthropic.com".into(),
        probe_interval: Duration::from_secs(2),
        probe_timeout: Duration::from_secs(1),
        probe_mode: ProbeMode::Http,
        health_url: "http://127.0.0.1:9989/api/health".into(),
        ca_dir: PathBuf::from("/tmp/ca"),
        direct_extra_ca: None,
        direct_target: None,
        allow_insecure_direct: false,
        max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    };
    let out = cfg.render();
    assert!(out.contains("bind"));
    assert!(out.contains("api.anthropic.com"));
    assert!(out.contains("/api/health"));
    assert!(out.contains("/tmp/ca"));
    assert!(out.contains("allow_insecure_direct"));
    assert!(out.contains("max_body_bytes"));
    Ok(())
}

#[test]
fn upstream_authority_parsed_from_upstream() -> anyhow::Result<()> {
    let cfg = config_from(&[], ConfigOverrides::default())?;
    assert_eq!(cfg.upstream_authority.as_str(), "127.0.0.1:9988");
    Ok(())
}

#[test]
fn malformed_upstream_is_rejected() -> anyhow::Result<()> {
    let err = config_error(&[("MIKE_FORWARD_UPSTREAM", "not a url")])?;
    assert!(err.contains("MIKE_FORWARD_UPSTREAM"));
    Ok(())
}

#[test]
fn insecure_direct_seam_without_ack_is_refused() -> anyhow::Result<()> {
    let err = config_error(&[("MIKE_FORWARD_DIRECT_TARGET", "127.0.0.1:1")])?;
    assert!(err.contains("refusing to start"));
    assert!(err.contains("MIKE_FORWARD_ALLOW_INSECURE_DIRECT"));
    Ok(())
}

#[test]
fn insecure_direct_seam_with_env_ack_is_allowed() -> anyhow::Result<()> {
    let cfg = config_from(
        &[
            ("MIKE_FORWARD_DIRECT_TARGET", "127.0.0.1:1"),
            ("MIKE_FORWARD_ALLOW_INSECURE_DIRECT", "1"),
        ],
        ConfigOverrides::default(),
    )?;
    assert!(cfg.allow_insecure_direct);
    assert_eq!(cfg.direct_target.as_deref(), Some("127.0.0.1:1"));
    Ok(())
}

#[test]
fn insecure_direct_seam_with_flag_ack_is_allowed() -> anyhow::Result<()> {
    let overrides = ConfigOverrides {
        allow_insecure_direct: true,
        ..Default::default()
    };
    let cfg = config_from(
        &[("MIKE_FORWARD_DIRECT_EXTRA_CA", "/tmp/ca.pem")],
        overrides,
    )?;
    assert!(cfg.allow_insecure_direct);
    Ok(())
}

#[test]
fn max_body_bytes_defaults_and_overrides() -> anyhow::Result<()> {
    let cfg = config_from(&[], ConfigOverrides::default())?;
    assert_eq!(cfg.max_body_bytes, DEFAULT_MAX_BODY_BYTES);

    let cfg = config_from(
        &[("MIKE_FORWARD_MAX_BODY_BYTES", "1024")],
        ConfigOverrides::default(),
    )?;
    assert_eq!(cfg.max_body_bytes, 1024);
    Ok(())
}

#[test]
fn bad_max_body_bytes_is_rejected() -> anyhow::Result<()> {
    let err = config_error(&[("MIKE_FORWARD_MAX_BODY_BYTES", "huge")])?;
    assert!(err.contains("MIKE_FORWARD_MAX_BODY_BYTES"));
    Ok(())
}

#[test]
fn ca_dir_explicit_beats_env_beats_default() {
    let with_env = lookup_of(&[("MIKE_FORWARD_CA_DIR", "/from/env")]);
    assert_eq!(
        resolve_ca_dir_from(Some(PathBuf::from("/explicit")), &with_env),
        PathBuf::from("/explicit")
    );
    assert_eq!(
        resolve_ca_dir_from(None, &with_env),
        PathBuf::from("/from/env")
    );
    assert_eq!(resolve_ca_dir_from(None, &lookup_of(&[])), default_ca_dir());
}

#[test]
fn ca_cert_path_is_the_cert_file_inside_the_resolved_directory() {
    let with_env = lookup_of(&[("MIKE_FORWARD_CA_DIR", "/from/env")]);
    assert_eq!(
        crate::ca::ca_cert_path(&resolve_ca_dir_from(None, &with_env)),
        PathBuf::from("/from/env/ca-cert.pem")
    );
    assert_eq!(
        crate::ca::ca_cert_path(std::path::Path::new("/x")),
        PathBuf::from("/x/ca-cert.pem")
    );
}
