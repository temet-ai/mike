use super::*;

const ALL_KEYS: &[&str] = &[
    "MIKE_FORWARD_BIND",
    "MIKE_FORWARD_UPSTREAM",
    "MIKE_FORWARD_INTERCEPT_HOST",
    "MIKE_FORWARD_HEALTH_URL",
    "MIKE_FORWARD_PROBE_INTERVAL",
    "MIKE_FORWARD_PROBE_TIMEOUT",
    "MIKE_FORWARD_PROBE_MODE",
    "MIKE_FORWARD_CA_DIR",
    "MIKE_FORWARD_DIRECT_TARGET",
    "MIKE_FORWARD_DIRECT_EXTRA_CA",
    "MIKE_FORWARD_ALLOW_INSECURE_DIRECT",
    "MIKE_FORWARD_MAX_BODY_BYTES",
];

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn clean() -> Self {
        let lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let guard = Self { _lock: lock };
        guard.clear();
        guard
    }

    fn set(pairs: &[(&str, &str)]) -> Self {
        let guard = Self::clean();
        guard.set_vars(pairs);
        guard
    }

    fn set_vars(&self, pairs: &[(&str, &str)]) {
        for (k, v) in pairs {
            unsafe { std::env::set_var(k, v) };
        }
    }

    fn clear(&self) {
        for k in ALL_KEYS {
            unsafe { std::env::remove_var(k) };
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        self.clear();
    }
}

#[test]
fn defaults_apply_when_env_and_overrides_absent() -> anyhow::Result<()> {
    let _g = EnvGuard::clean();
    let cfg = Config::from_env_and_overrides(ConfigOverrides::default())
        .map_err(|e| anyhow::anyhow!(e))?;
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
    let _g = EnvGuard::set(&[
        ("MIKE_FORWARD_BIND", "0.0.0.0:1234"),
        (
            "MIKE_FORWARD_HEALTH_URL",
            "http://localhost:7777/api/health",
        ),
        ("MIKE_FORWARD_PROBE_INTERVAL", "5s"),
    ]);
    let cfg = Config::from_env_and_overrides(ConfigOverrides::default())
        .map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(cfg.bind, "0.0.0.0:1234");
    assert_eq!(cfg.health_url, "http://localhost:7777/api/health");
    assert_eq!(cfg.probe_interval, Duration::from_secs(5));
    Ok(())
}

#[test]
fn flag_override_beats_env() -> anyhow::Result<()> {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_BIND", "0.0.0.0:1234")]);
    let overrides = ConfigOverrides {
        bind: Some("127.0.0.1:5555".to_string()),
        ..Default::default()
    };
    let cfg = Config::from_env_and_overrides(overrides).map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(cfg.bind, "127.0.0.1:5555");
    Ok(())
}

#[test]
fn bad_duration_is_rejected() {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_PROBE_INTERVAL", "not-a-duration")]);
    let err = match Config::from_env_and_overrides(ConfigOverrides::default()) {
        Err(e) => e,
        Ok(_) => panic!("invalid duration must error"),
    };
    assert!(err.contains("MIKE_FORWARD_PROBE_INTERVAL"));
    assert!(err.contains("not a valid duration"));
}

#[test]
fn unknown_probe_mode_is_rejected() {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_PROBE_MODE", "ping")]);
    let err = match Config::from_env_and_overrides(ConfigOverrides::default()) {
        Err(e) => e,
        Ok(_) => panic!("unknown probe mode must error"),
    };
    assert!(err.contains("unknown probe mode"));
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
    let _g = EnvGuard::clean();
    let cfg = Config::from_env_and_overrides(ConfigOverrides::default())
        .map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(cfg.upstream_authority.as_str(), "127.0.0.1:9988");
    Ok(())
}

#[test]
fn malformed_upstream_is_rejected() {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_UPSTREAM", "not a url")]);
    let err = match Config::from_env_and_overrides(ConfigOverrides::default()) {
        Err(e) => e,
        Ok(_) => panic!("malformed upstream must error"),
    };
    assert!(err.contains("MIKE_FORWARD_UPSTREAM"));
}

#[test]
fn insecure_direct_seam_without_ack_is_refused() {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_DIRECT_TARGET", "127.0.0.1:1")]);
    let err = match Config::from_env_and_overrides(ConfigOverrides::default()) {
        Err(e) => e,
        Ok(_) => panic!("insecure direct seam without ack must refuse to start"),
    };
    assert!(err.contains("refusing to start"));
    assert!(err.contains("MIKE_FORWARD_ALLOW_INSECURE_DIRECT"));
}

#[test]
fn insecure_direct_seam_with_env_ack_is_allowed() -> anyhow::Result<()> {
    let _g = EnvGuard::set(&[
        ("MIKE_FORWARD_DIRECT_TARGET", "127.0.0.1:1"),
        ("MIKE_FORWARD_ALLOW_INSECURE_DIRECT", "1"),
    ]);
    let cfg = Config::from_env_and_overrides(ConfigOverrides::default())
        .map_err(|e| anyhow::anyhow!(e))?;
    assert!(cfg.allow_insecure_direct);
    assert_eq!(cfg.direct_target.as_deref(), Some("127.0.0.1:1"));
    Ok(())
}

#[test]
fn insecure_direct_seam_with_flag_ack_is_allowed() -> anyhow::Result<()> {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_DIRECT_EXTRA_CA", "/tmp/ca.pem")]);
    let overrides = ConfigOverrides {
        allow_insecure_direct: true,
        ..Default::default()
    };
    let cfg = Config::from_env_and_overrides(overrides).map_err(|e| anyhow::anyhow!(e))?;
    assert!(cfg.allow_insecure_direct);
    Ok(())
}

#[test]
fn max_body_bytes_defaults_and_overrides() -> anyhow::Result<()> {
    let guard = EnvGuard::clean();
    let cfg = Config::from_env_and_overrides(ConfigOverrides::default())
        .map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(cfg.max_body_bytes, DEFAULT_MAX_BODY_BYTES);

    guard.set_vars(&[("MIKE_FORWARD_MAX_BODY_BYTES", "1024")]);
    let cfg = Config::from_env_and_overrides(ConfigOverrides::default())
        .map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(cfg.max_body_bytes, 1024);
    Ok(())
}

#[test]
fn bad_max_body_bytes_is_rejected() {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_MAX_BODY_BYTES", "huge")]);
    let err = match Config::from_env_and_overrides(ConfigOverrides::default()) {
        Err(e) => e,
        Ok(_) => panic!("invalid byte count must error"),
    };
    assert!(err.contains("MIKE_FORWARD_MAX_BODY_BYTES"));
}

#[test]
fn ca_dir_explicit_beats_env_beats_default() {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_CA_DIR", "/from/env")]);
    assert_eq!(
        resolve_ca_dir(Some(PathBuf::from("/explicit"))),
        PathBuf::from("/explicit")
    );
    assert_eq!(resolve_ca_dir(None), PathBuf::from("/from/env"));
    drop(_g);
    let _clean = EnvGuard::clean();
    assert_eq!(resolve_ca_dir(None), default_ca_dir());
}

#[test]
fn ca_cert_path_is_the_cert_file_inside_the_resolved_directory() {
    let _g = EnvGuard::set(&[("MIKE_FORWARD_CA_DIR", "/from/env")]);
    assert_eq!(
        crate::ca::default_ca_cert_path(),
        PathBuf::from("/from/env/ca-cert.pem")
    );
    assert_eq!(
        crate::ca::ca_cert_path(std::path::Path::new("/x")),
        PathBuf::from("/x/ca-cert.pem")
    );
}
