use anyhow::Context;

use super::redact::redact_database_url;
use crate::config::ProxyConfig;

pub(crate) fn to_toml_string(config: &ProxyConfig) -> anyhow::Result<String> {
    let mut redacted = config.clone();
    if let Some(url) = redacted.database_url.as_deref() {
        redacted.database_url = Some(redact_database_url(url));
    }
    if let Some(url) = redacted.local_database_url.as_deref() {
        redacted.local_database_url = Some(redact_database_url(url));
    }
    toml::to_string_pretty(&redacted).context("failed to serialise config to TOML")
}

fn push_export(out: &mut String, key: &str, value: String) {
    out.push_str(&format!("{key}={}\n", shell_quote(&value)));
}

fn push_export_opt(out: &mut String, key: &str, value: Option<String>) {
    if let Some(v) = value {
        push_export(out, key, v);
    }
}

pub(crate) fn to_shell_exports(config: &ProxyConfig) -> String {
    let mut out = String::new();
    push_network_exports(&mut out, config);
    push_store_capacity_exports(&mut out, config);
    push_observability_exports(&mut out, config);
    push_persistence_exports(&mut out, config);
    push_retention_exports(&mut out, config);
    push_frontend_exports(&mut out, config);
    push_body_budget_exports(&mut out, config);
    push_mailbox_exports(&mut out, config);
    out
}

fn push_network_exports(out: &mut String, config: &ProxyConfig) {
    push_export(out, "MIKE_LISTEN_ADDR", config.proxy_listen_addr.clone());
    push_export(
        out,
        "MIKE_UNSUPPORTED_ALLOW_REMOTE_LISTENERS",
        config.unsupported_allow_remote_listeners.to_string(),
    );
    push_export(out, "MIKE_INSPECT_ADDR", config.inspect_listen_addr.clone());
    push_export(out, "MIKE_UPSTREAM_URL", config.upstream_url.clone());
}

fn push_store_capacity_exports(out: &mut String, config: &ProxyConfig) {
    push_export(out, "MIKE_MAX_REQUESTS", config.max_requests.to_string());
    push_export(out, "MIKE_MAX_AGE", config.max_age.clone());
}

fn push_observability_exports(out: &mut String, config: &ProxyConfig) {
    push_export_opt(out, "MIKE_OTEL_ENDPOINT", config.otel_endpoint.clone());
    push_export(out, "MIKE_LOG_FORMAT", config.log_format.clone());
}

fn push_persistence_exports(out: &mut String, config: &ProxyConfig) {
    push_export(
        out,
        "MIKE_PERSIST_ENABLED",
        if config.persist_enabled { "1" } else { "0" }.to_string(),
    );
    push_export(
        out,
        "MIKE_DB_FALLBACK_ENABLED",
        if config.db_fallback_enabled { "1" } else { "0" }.to_string(),
    );
    push_export(
        out,
        "MIKE_DB_RESYNC_ENABLED",
        if config.db_resync_enabled { "1" } else { "0" }.to_string(),
    );
    push_export_opt(
        out,
        "MIKE_DATA_DIR",
        config
            .data_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    );
    push_export_opt(
        out,
        "MIKE_PROJECT_PINNED",
        config
            .project_pinned
            .map(|b| if b { "1" } else { "0" }.to_string()),
    );
}

fn push_retention_exports(out: &mut String, config: &ProxyConfig) {
    push_export(
        out,
        "MIKE_DEFAULT_TTL_SECS",
        config.default_ttl_secs.to_string(),
    );
    push_export(out, "MIKE_DEFAULT_TTL", config.default_ttl.clone());
    push_export(
        out,
        "MIKE_SWEEP_AT_STARTUP",
        if config.sweep_at_startup { "1" } else { "0" }.to_string(),
    );
}

fn push_frontend_exports(out: &mut String, config: &ProxyConfig) {
    push_export(out, "MIKE_FRONTEND_PORT", config.frontend_port.to_string());
    push_export(
        out,
        "MIKE_COMPOSITION_ENABLED",
        if config.composition_enabled { "1" } else { "0" }.to_string(),
    );
    push_export(out, "MIKE_SPA_ORIGINS", config.spa_origins.join(","));
    push_export(
        out,
        "MIKE_INSPECT_ALLOWED_HOSTS",
        config.inspect_allowed_hosts.join(","),
    );
    push_export(
        out,
        "MIKE_SPA_ENABLED",
        if config.spa_enabled { "1" } else { "0" }.to_string(),
    );
    push_export(out, "MIKE_SPA_BIND", config.spa_bind.clone());
}

fn push_body_budget_exports(out: &mut String, config: &ProxyConfig) {
    push_export(
        out,
        "MIKE_MAX_REQUEST_BODY_BYTES",
        config.max_request_body_bytes.to_string(),
    );
    push_export(
        out,
        "MIKE_MAX_RESPONSE_BODY_BYTES",
        config.max_response_body_bytes.to_string(),
    );
    push_export(
        out,
        "MIKE_MAX_STORE_BYTES",
        config.max_store_bytes.to_string(),
    );
}

fn push_mailbox_exports(out: &mut String, config: &ProxyConfig) {
    push_export(
        out,
        "MIKE_MAILBOX_STALE_MINUTES",
        config.mailbox_stale_minutes.to_string(),
    );
    push_export(
        out,
        "MIKE_MAILBOX_DELIVERY_TTL_MINUTES",
        config.mailbox_delivery_ttl_minutes.to_string(),
    );
    push_export(
        out,
        "MIKE_MAILBOX_MAX_DELIVERIES_PER_TURN",
        config.mailbox_max_deliveries_per_turn.to_string(),
    );
    push_export(
        out,
        "MIKE_MAILBOX_REJECT_SEND_TO_ENDED_SESSION",
        if config.mailbox_reject_send_to_ended_session {
            "1"
        } else {
            "0"
        }
        .to_string(),
    );
}

fn shell_quote(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('\'');
    for ch in raw.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load::{CliOverrides, load_layered};
    use crate::environment::MapEnvironment;

    #[test]
    fn to_toml_string_redacts_local_database_url_password() -> anyhow::Result<()> {
        let config = ProxyConfig {
            local_database_url: Some(
                "postgres://proxy:hunter2@localhost:6666/claude_proxy_local".to_string(),
            ),
            ..ProxyConfig::default()
        };
        let body = to_toml_string(&config)?;
        assert!(
            body.contains("postgres://proxy:***@localhost:6666/claude_proxy_local"),
            "expected redacted local URL in TOML: {body}"
        );
        assert!(
            !body.contains("hunter2"),
            "local DB password must not leak into TOML: {body}"
        );
        Ok(())
    }

    #[test]
    fn shell_exports_omits_local_database_url_but_emits_db_gates() {
        let config = ProxyConfig {
            local_database_url: Some(
                "postgres://proxy:hunter2@localhost:6666/claude_proxy_local".to_string(),
            ),
            ..ProxyConfig::default()
        };
        let out = to_shell_exports(&config);
        assert!(
            !out.contains("MIKE_LOCAL_DATABASE_URL"),
            "MIKE_LOCAL_DATABASE_URL must not appear in shell exports: {out}"
        );
        assert!(
            !out.contains("hunter2"),
            "local DB password must not leak into shell output: {out}"
        );
        assert!(
            out.contains("MIKE_DB_FALLBACK_ENABLED='1'"),
            "MIKE_DB_FALLBACK_ENABLED must appear in shell exports: {out}"
        );
        assert!(
            out.contains("MIKE_DB_RESYNC_ENABLED='1'"),
            "MIKE_DB_RESYNC_ENABLED must appear in shell exports: {out}"
        );
    }

    #[test]
    fn to_shell_exports_emits_key_value_lines() {
        let config = ProxyConfig::default();
        let out = to_shell_exports(&config);
        assert!(out.contains("MIKE_LISTEN_ADDR='127.0.0.1:9988'"));
        assert!(out.contains("MIKE_INSPECT_ADDR='127.0.0.1:9989'"));
        assert!(out.contains("MIKE_PERSIST_ENABLED='1'"));
        assert!(
            !out.contains("MIKE_EMBEDDING"),
            "embedding vars must no longer be exported to the runtime env"
        );
        assert!(
            !out.contains("MIKE_DATABASE_URL"),
            "unset optional fields must be omitted in shell output"
        );
    }

    #[test]
    fn to_toml_string_round_trips() -> anyhow::Result<()> {
        let config = ProxyConfig::default();
        let body = to_toml_string(&config)?;
        assert!(body.contains("proxy_listen_addr"));
        assert!(body.contains("persist_enabled"));
        let parsed = load_layered(CliOverrides::default(), &MapEnvironment::isolated())?;
        assert_eq!(parsed.proxy_listen_addr, config.proxy_listen_addr);
        assert_eq!(parsed.persist_enabled, config.persist_enabled);
        Ok(())
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("simple"), "'simple'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn shell_exports_omits_database_url() {
        let config = ProxyConfig {
            database_url: Some("postgres://proxy:hunter2@localhost:5432/claude_proxy".to_string()),
            ..ProxyConfig::default()
        };
        let out = to_shell_exports(&config);
        assert!(
            !out.contains("MIKE_DATABASE_URL"),
            "MIKE_DATABASE_URL must not appear in shell exports: {out}"
        );
        assert!(
            !out.contains("hunter2"),
            "password `hunter2` must not leak into shell output: {out}"
        );
    }

    #[test]
    fn to_toml_string_redacts_database_url_password() -> anyhow::Result<()> {
        let config = ProxyConfig {
            database_url: Some("postgres://proxy:hunter2@localhost:5432/claude_proxy".to_string()),
            ..ProxyConfig::default()
        };
        let body = to_toml_string(&config)?;
        assert!(
            body.contains("postgres://proxy:***@localhost:5432/claude_proxy"),
            "expected redacted URL in TOML: {body}"
        );
        assert!(
            !body.contains("hunter2"),
            "password must not leak into TOML: {body}"
        );
        Ok(())
    }

    fn parse_shell_quoted(raw: &str) -> String {
        let mut out = String::new();
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\'' => {
                    while let Some(&next) = chars.peek() {
                        if next == '\'' {
                            chars.next();
                            break;
                        }
                        out.push(next);
                        chars.next();
                    }
                }
                '\\' => {
                    if let Some(&next) = chars.peek() {
                        out.push(next);
                        chars.next();
                    }
                }
                other => out.push(other),
            }
        }
        out
    }

    #[test]
    fn to_shell_exports_emits_spa_origins() {
        let config = ProxyConfig {
            spa_origins: vec![
                "http://127.0.0.1:9990".to_string(),
                "https://ops.example.com".to_string(),
            ],
            ..ProxyConfig::default()
        };
        let out = to_shell_exports(&config);
        assert!(
            out.contains("MIKE_SPA_ORIGINS='http://127.0.0.1:9990,https://ops.example.com'"),
            "expected MIKE_SPA_ORIGINS line in shell output: {out}"
        );
    }

    #[test]
    fn to_shell_exports_roundtrip_spa_origins() -> anyhow::Result<()> {
        let original = ProxyConfig {
            spa_origins: vec![
                "http://127.0.0.1:9990".to_string(),
                "https://ops.example.com".to_string(),
                "http://192.168.1.10:9990".to_string(),
            ],
            ..ProxyConfig::default()
        };
        let shell = to_shell_exports(&original);
        let line = shell
            .lines()
            .find(|l| l.starts_with("MIKE_SPA_ORIGINS="))
            .ok_or_else(|| anyhow::anyhow!("MIKE_SPA_ORIGINS missing from shell output"))?;
        let raw_value = line
            .strip_prefix("MIKE_SPA_ORIGINS=")
            .ok_or_else(|| anyhow::anyhow!("unexpected line format"))?;
        let unquoted = parse_shell_quoted(raw_value);
        assert_eq!(unquoted, original.spa_origins.join(","));

        let environment = MapEnvironment::isolated().with("MIKE_SPA_ORIGINS", &unquoted);
        let loaded = load_layered(CliOverrides::default(), &environment)?;
        assert_eq!(
            loaded.spa_origins, original.spa_origins,
            "spa_origins must round-trip through to_shell_exports → env → load"
        );
        Ok(())
    }
}
