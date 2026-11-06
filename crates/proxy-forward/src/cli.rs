use anyhow::{Context, bail};
use tracing_subscriber::EnvFilter;

use crate::ca::Ca;
use crate::config::{Config, ConfigOverrides};

const USAGE: &str = "use `serve` (default), `export-ca`, or `print-config`";

pub async fn run_cli(args: Vec<String>) -> anyhow::Result<()> {
    let mut args = args.into_iter();
    let mut overrides = ConfigOverrides::default();

    let subcommand = match args.next() {
        Some(s) if s == "serve" || s == "export-ca" || s == "print-config" => s,
        Some(s) if s.starts_with("--") => {
            apply_flag(&mut overrides, &s, &mut args)?;
            "serve".to_string()
        }
        Some(other) => bail!("unknown subcommand: {other}; {USAGE}"),
        None => "serve".to_string(),
    };

    while let Some(flag) = args.next() {
        apply_flag(&mut overrides, &flag, &mut args)?;
    }

    let config = Config::from_env_and_overrides(overrides).map_err(|e| anyhow::anyhow!(e))?;

    init_tracing();

    match subcommand.as_str() {
        "export-ca" => export_ca(&config),
        "print-config" => {
            eprint!("{}", config.render());
            Ok(())
        }
        _ => serve(config).await,
    }
}

fn export_ca(config: &Config) -> anyhow::Result<()> {
    let ca = Ca::load_or_generate(&config.ca_dir)
        .with_context(|| format!("load/generate CA in {}", config.ca_dir.display()))?;
    print!("{}", ca.cert_pem());
    Ok(())
}

async fn serve(config: Config) -> anyhow::Result<()> {
    log_starting(&config);
    warn_if_insecure_direct(&config);
    crate::run(config)
        .await
        .context("proxy-forward server loop exited with an error")?;
    tracing::info!("proxy-forward exiting normally");
    Ok(())
}

fn log_starting(config: &Config) {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        bind = %config.bind,
        "proxy-forward starting"
    );
}

fn warn_if_insecure_direct(config: &Config) {
    if !config.allow_insecure_direct
        || (config.direct_target.is_none() && config.direct_extra_ca.is_none())
    {
        return;
    }
    tracing::warn!(
        direct_target = ?config.direct_target,
        direct_extra_ca = ?config.direct_extra_ca,
        "INSECURE direct-leg seam is ACTIVE (redirected dial and/or \
         extra-trusted CA) — this is a MITM surface, test/corporate-MITM \
         only. Do NOT run this configuration against real Anthropic traffic."
    );
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_target(true)
        .try_init();
}

fn flag_value(
    flag: &str,
    inline: Option<&str>,
    args: &mut impl Iterator<Item = String>,
) -> anyhow::Result<String> {
    if let Some(v) = inline {
        return Ok(v.to_string());
    }
    args.next()
        .with_context(|| format!("flag {flag} requires a value"))
}

fn apply_flag(
    overrides: &mut ConfigOverrides,
    flag: &str,
    args: &mut impl Iterator<Item = String>,
) -> anyhow::Result<()> {
    let (name, inline) = match flag.split_once('=') {
        Some((n, v)) => (n, Some(v)),
        None => (flag, None),
    };

    match name {
        "--bind" => overrides.bind = Some(flag_value(name, inline, args)?),
        "--upstream" => overrides.upstream = Some(flag_value(name, inline, args)?),
        "--intercept-host" => overrides.intercept_host = Some(flag_value(name, inline, args)?),
        "--health-url" => overrides.health_url = Some(flag_value(name, inline, args)?),
        "--ca-dir" => overrides.ca_dir = Some(flag_value(name, inline, args)?.into()),
        "--direct-extra-ca" => {
            overrides.direct_extra_ca = Some(flag_value(name, inline, args)?.into());
        }
        "--direct-target" => overrides.direct_target = Some(flag_value(name, inline, args)?),
        "--allow-insecure-direct" => overrides.allow_insecure_direct = true,
        "--max-body-bytes" => overrides.max_body_bytes = Some(flag_value(name, inline, args)?),
        other => bail!(
            "unknown flag: {other} (supported: --bind, --upstream, --intercept-host, \
             --health-url, --ca-dir, --direct-extra-ca, --direct-target, \
             --allow-insecure-direct, --max-body-bytes)"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_flag_reads_inline_and_separate_values() -> anyhow::Result<()> {
        let mut overrides = ConfigOverrides::default();
        apply_flag(
            &mut overrides,
            "--bind=127.0.0.1:1",
            &mut std::iter::empty(),
        )?;
        let mut rest = vec!["http://x".to_string()].into_iter();
        apply_flag(&mut overrides, "--upstream", &mut rest)?;
        assert_eq!(overrides.bind.as_deref(), Some("127.0.0.1:1"));
        assert_eq!(overrides.upstream.as_deref(), Some("http://x"));
        Ok(())
    }

    #[test]
    fn apply_flag_rejects_unknown_flags_and_missing_values() {
        let mut overrides = ConfigOverrides::default();
        assert!(apply_flag(&mut overrides, "--nope", &mut std::iter::empty()).is_err());
        assert!(apply_flag(&mut overrides, "--bind", &mut std::iter::empty()).is_err());
    }
}
