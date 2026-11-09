use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMode {
    Http,
}

impl ProbeMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "http" | "http_health" => Ok(ProbeMode::Http),
            other => Err(format!("unknown probe mode {other:?} (supported: http)")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub upstream: String,
    pub upstream_authority: http::uri::Authority,
    pub intercept_host: String,
    pub probe_interval: Duration,
    pub probe_timeout: Duration,
    pub probe_mode: ProbeMode,
    pub health_url: String,
    pub ca_dir: PathBuf,
    pub direct_extra_ca: Option<PathBuf>,
    pub direct_target: Option<String>,
    pub allow_insecure_direct: bool,
    pub max_body_bytes: usize,
}

pub const DEFAULT_BIND: &str = "127.0.0.1:9987";
pub const DEFAULT_UPSTREAM: &str = "http://127.0.0.1:9988";
pub const DEFAULT_INTERCEPT_HOST: &str = "api.anthropic.com";
pub const DEFAULT_HEALTH_URL: &str = "http://127.0.0.1:9989/api/health";
pub const DEFAULT_PROBE_INTERVAL: Duration = Duration::from_secs(2);
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(1);
pub const DEFAULT_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

impl Config {
    pub fn from_env_and_overrides(overrides: ConfigOverrides) -> Result<Self, String> {
        Self::from_lookup(overrides, &process_lookup)
    }

    pub fn from_lookup(overrides: ConfigOverrides, lookup: Lookup<'_>) -> Result<Self, String> {
        let env_nonempty = |key: &str| env_nonempty(lookup, key);
        let bind = overrides
            .bind
            .or_else(|| env_nonempty("MIKE_FORWARD_BIND"))
            .unwrap_or_else(|| DEFAULT_BIND.to_string());

        let upstream = overrides
            .upstream
            .or_else(|| env_nonempty("MIKE_FORWARD_UPSTREAM"))
            .unwrap_or_else(|| DEFAULT_UPSTREAM.to_string());

        let intercept_host = overrides
            .intercept_host
            .or_else(|| env_nonempty("MIKE_FORWARD_INTERCEPT_HOST"))
            .unwrap_or_else(|| DEFAULT_INTERCEPT_HOST.to_string());

        let health_url = overrides
            .health_url
            .or_else(|| env_nonempty("MIKE_FORWARD_HEALTH_URL"))
            .unwrap_or_else(|| DEFAULT_HEALTH_URL.to_string());

        let probe_interval = match env_nonempty("MIKE_FORWARD_PROBE_INTERVAL") {
            Some(raw) => parse_duration(&raw, "MIKE_FORWARD_PROBE_INTERVAL")?,
            None => DEFAULT_PROBE_INTERVAL,
        };

        let probe_timeout = match env_nonempty("MIKE_FORWARD_PROBE_TIMEOUT") {
            Some(raw) => parse_duration(&raw, "MIKE_FORWARD_PROBE_TIMEOUT")?,
            None => DEFAULT_PROBE_TIMEOUT,
        };

        let probe_mode = match env_nonempty("MIKE_FORWARD_PROBE_MODE") {
            Some(raw) => ProbeMode::parse(&raw)?,
            None => ProbeMode::Http,
        };

        let ca_dir = resolve_ca_dir(overrides.ca_dir);

        let direct_extra_ca = overrides
            .direct_extra_ca
            .or_else(|| env_nonempty("MIKE_FORWARD_DIRECT_EXTRA_CA").map(PathBuf::from));

        let direct_target = overrides
            .direct_target
            .or_else(|| env_nonempty("MIKE_FORWARD_DIRECT_TARGET"));

        let max_body_bytes = match overrides
            .max_body_bytes
            .or_else(|| env_nonempty("MIKE_FORWARD_MAX_BODY_BYTES"))
        {
            Some(raw) => parse_byte_size(&raw, "MIKE_FORWARD_MAX_BODY_BYTES")?,
            None => DEFAULT_MAX_BODY_BYTES,
        };

        let upstream_authority = parse_upstream_authority(&upstream)?;

        let allow_insecure_direct = overrides.allow_insecure_direct
            || matches!(
                env_nonempty("MIKE_FORWARD_ALLOW_INSECURE_DIRECT").as_deref(),
                Some("1")
            );
        if (direct_target.is_some() || direct_extra_ca.is_some()) && !allow_insecure_direct {
            return Err("refusing to start: MIKE_FORWARD_DIRECT_TARGET / \
                 MIKE_FORWARD_DIRECT_EXTRA_CA (or --direct-target / \
                 --direct-extra-ca) is set, but it is an insecure direct-leg \
                 seam (a redirected dial or an extra-trusted CA is a MITM \
                 surface, test/corporate-MITM only). Set \
                 MIKE_FORWARD_ALLOW_INSECURE_DIRECT=1 (or pass \
                 --allow-insecure-direct) to acknowledge and enable it."
                .to_string());
        }

        Ok(Self {
            bind,
            upstream,
            upstream_authority,
            intercept_host,
            probe_interval,
            probe_timeout,
            probe_mode,
            health_url,
            ca_dir,
            direct_extra_ca,
            direct_target,
            allow_insecure_direct,
            max_body_bytes,
        })
    }

    pub fn render(&self) -> String {
        format!(
            "proxy-forward resolved configuration:\n\
             \x20 bind            = {bind}\n\
             \x20 upstream        = {upstream}\n\
             \x20 intercept_host  = {intercept_host}\n\
             \x20 probe_mode      = {probe_mode:?}\n\
             \x20 health_url      = {health_url}\n\
             \x20 probe_interval  = {probe_interval}\n\
             \x20 probe_timeout   = {probe_timeout}\n\
             \x20 ca_dir          = {ca_dir}\n\
             \x20 direct_extra_ca = {direct_extra_ca}\n\
             \x20 direct_target   = {direct_target}\n\
             \x20 allow_insecure_direct = {allow_insecure_direct}\n\
             \x20 max_body_bytes  = {max_body_bytes}\n",
            bind = self.bind,
            upstream = self.upstream,
            intercept_host = self.intercept_host,
            probe_mode = self.probe_mode,
            health_url = self.health_url,
            probe_interval = humantime::format_duration(self.probe_interval),
            probe_timeout = humantime::format_duration(self.probe_timeout),
            ca_dir = self.ca_dir.display(),
            direct_extra_ca = self
                .direct_extra_ca
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(none)".to_string()),
            direct_target = self.direct_target.as_deref().unwrap_or("(none)"),
            allow_insecure_direct = self.allow_insecure_direct,
            max_body_bytes = self.max_body_bytes,
        )
    }
}

#[derive(Debug, Default, Clone)]
pub struct ConfigOverrides {
    pub bind: Option<String>,
    pub upstream: Option<String>,
    pub intercept_host: Option<String>,
    pub health_url: Option<String>,
    pub ca_dir: Option<PathBuf>,
    pub direct_extra_ca: Option<PathBuf>,
    pub direct_target: Option<String>,
    pub allow_insecure_direct: bool,
    pub max_body_bytes: Option<String>,
}

pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

pub fn process_lookup(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn env_nonempty(lookup: Lookup<'_>, key: &str) -> Option<String> {
    lookup(key).filter(|value| !value.trim().is_empty())
}

fn parse_duration(raw: &str, label: &str) -> Result<Duration, String> {
    humantime::parse_duration(raw.trim())
        .map_err(|e| format!("{label}={raw:?} is not a valid duration: {e}"))
}

fn parse_byte_size(raw: &str, label: &str) -> Result<usize, String> {
    raw.trim()
        .parse::<usize>()
        .map_err(|e| format!("{label}={raw:?} is not a valid byte count: {e}"))
}

fn parse_upstream_authority(upstream: &str) -> Result<http::uri::Authority, String> {
    let uri: http::Uri = upstream
        .parse()
        .map_err(|e| format!("MIKE_FORWARD_UPSTREAM={upstream:?} is not a valid URL: {e}"))?;
    uri.authority()
        .cloned()
        .ok_or_else(|| format!("MIKE_FORWARD_UPSTREAM={upstream:?} has no host:port authority"))
}

pub fn resolve_ca_dir(explicit: Option<PathBuf>) -> PathBuf {
    resolve_ca_dir_from(explicit, &process_lookup)
}

pub fn resolve_ca_dir_from(explicit: Option<PathBuf>, lookup: Lookup<'_>) -> PathBuf {
    explicit
        .or_else(|| env_nonempty(lookup, "MIKE_FORWARD_CA_DIR").map(PathBuf::from))
        .unwrap_or_else(default_ca_dir)
}

pub fn default_ca_dir() -> PathBuf {
    directories::ProjectDirs::from("", "temet", "proxy-forward")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests;
