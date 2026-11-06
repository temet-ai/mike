use anyhow::{Context, bail};

use crate::config::{CliOverrides, ProxyConfig};

pub async fn run(args: Vec<String>, overrides: CliOverrides) -> anyhow::Result<()> {
    if args.first().map(String::as_str) == Some("ca-path") {
        return print_ca_path(overrides);
    }
    let serving = serves(&args);
    let mut args = args;
    if serving {
        let config = crate::config::load(overrides).context("failed to load configuration")?;
        args = with_config_defaults(args, &config, &|name| std::env::var(name).ok());
    }
    proxy_forward::cli::run_cli(args).await
}

fn serves(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        None | Some("serve") => true,
        Some(first) => first.starts_with("--"),
    }
}

fn print_ca_path(overrides: CliOverrides) -> anyhow::Result<()> {
    let config = crate::config::load(overrides).context("failed to load configuration")?;
    let cwd = std::env::current_dir().ok();
    match crate::launch::cli::resolve_ca_cert(&config.launch, cwd.as_deref()) {
        Some(path) => {
            println!("{}", path.display());
            Ok(())
        }
        None => {
            let looked = crate::launch::cli::ca_cert_candidates(&config.launch, cwd.as_deref());
            bail!(
                "no sidecar CA certificate exists yet (looked at {}); `mike forward export-ca` creates one",
                looked
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

pub(crate) fn with_config_defaults(
    args: Vec<String>,
    config: &ProxyConfig,
    env: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut args = args;
    if args.first().map(String::as_str) != Some("serve") && !serves_with_flags(&args) {
        args.insert(0, "serve".to_string());
    }
    let defaults = [
        (
            "--bind",
            "MIKE_FORWARD_BIND",
            bind_of(&config.launch.proxy_forward_url),
        ),
        (
            "--upstream",
            "MIKE_FORWARD_UPSTREAM",
            Some(format!(
                "http://{}",
                connect_host(&config.proxy_listen_addr)
            )),
        ),
        (
            "--health-url",
            "MIKE_FORWARD_HEALTH_URL",
            Some(format!(
                "http://{}/api/health",
                connect_host(&config.inspect_listen_addr)
            )),
        ),
    ];
    for (flag, variable, value) in defaults {
        let given = args
            .iter()
            .any(|a| a == flag || a.starts_with(&format!("{flag}=")));
        if let (false, None, Some(value)) = (given, env(variable), value) {
            args.push(flag.to_string());
            args.push(value);
        }
    }
    args
}

fn serves_with_flags(args: &[String]) -> bool {
    args.first().is_some_and(|a| a.starts_with("--"))
}

pub(crate) fn bind_of(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    Some(format!("{}:{}", parsed.host_str()?, parsed.port()?))
}

fn connect_host(addr: &str) -> String {
    addr.replace("0.0.0.0", "127.0.0.1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn serving_defaults_follow_the_mike_config() -> anyhow::Result<()> {
        let config: ProxyConfig = serde_json::from_value(serde_json::json!({
            "proxy_listen_addr": "127.0.0.1:7001",
            "inspect_listen_addr": "0.0.0.0:7002",
        }))?;
        let args = with_config_defaults(Vec::new(), &config, &no_env);
        assert_eq!(
            args,
            [
                "serve",
                "--bind",
                "127.0.0.1:9987",
                "--upstream",
                "http://127.0.0.1:7001",
                "--health-url",
                "http://127.0.0.1:7002/api/health"
            ]
        );
        Ok(())
    }

    #[test]
    fn a_flag_or_variable_the_user_gave_is_not_overridden() {
        let config = ProxyConfig::default();
        let args = with_config_defaults(
            vec!["--upstream".to_string(), "http://elsewhere:1".to_string()],
            &config,
            &|name| (name == "MIKE_FORWARD_BIND").then(|| "0.0.0.0:1".to_string()),
        );
        assert_eq!(
            args.iter().filter(|a| a.as_str() == "--upstream").count(),
            1
        );
        assert!(!args.iter().any(|a| a == "--bind"));
        assert!(args.iter().any(|a| a == "http://elsewhere:1"));
    }

    #[test]
    fn only_serving_invocations_take_config_defaults() {
        assert!(serves(&[]));
        assert!(serves(&["serve".to_string()]));
        assert!(serves(&["--bind".to_string(), "x".to_string()]));
        assert!(!serves(&["export-ca".to_string()]));
        assert!(!serves(&["print-config".to_string()]));
    }
}
