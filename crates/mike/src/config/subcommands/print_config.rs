use anyhow::{Context, Result};
use serde_json::Value;

use super::PrintConfigArgs;
use crate::config::{CliOverrides, ProxyConfig, load};
use crate::store::persistence::PersistHandle;

const REDACTED: &str = "<redacted>";

const BOOT_HEADER: &str = "# BOOT CONFIG: defaults < selected TOML < MIKE_* env < CLI flags. This is NOT what a running\n\
# session got. Runtime settings and feature toggles are stored in Postgres and\n\
# override the boot values; they are listed under LIVE OVERLAY below.\n";

pub(super) struct LiveOverlay {
    settings: Vec<(String, Value)>,
    toggles: Vec<(String, bool, bool)>,
}

pub(super) enum OverlayState {
    Live(LiveOverlay),
    Unavailable(String),
}

pub(super) async fn print_config(args: PrintConfigArgs, cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for print-config")?;
    if args.paths {
        return print_paths(&config);
    }
    if args.shell {
        print!("{}", load::to_shell_exports(&config));
        return Ok(());
    }
    let boot = load::to_toml_string(&config)?;
    let overlay = read_live_overlay(&config).await;
    print!("{}", crate::user_config::render_sources());
    print!("{}", render_report(&boot, &overlay));
    Ok(())
}

fn print_paths(config: &ProxyConfig) -> Result<()> {
    let paths = crate::config::paths::InstallationPaths::resolve(Some(&config.config_path))?;
    let report = serde_json::json!({
        "config_file": paths.config_file,
        "installation_id": paths.installation_id,
        "data_dir": config.data_dir.as_ref().unwrap_or(&paths.data_dir),
        "state_dir": paths.state_dir,
        "log_dir": config.log_dir.as_ref().unwrap_or(&paths.log_dir),
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn read_live_overlay(config: &ProxyConfig) -> OverlayState {
    let Some(database_url) = config.database_url.as_deref() else {
        return OverlayState::Unavailable("no database_url is configured".to_owned());
    };
    match fetch_overlay(database_url).await {
        Ok(overlay) => OverlayState::Live(overlay),
        Err(_) => {
            OverlayState::Unavailable("database could not be read; credentials omitted".to_owned())
        }
    }
}

async fn fetch_overlay(database_url: &str) -> Result<LiveOverlay> {
    let persist = PersistHandle::connect_read_only(database_url)
        .await
        .context("could not connect to the database")?;
    let overlay = query_overlay(&persist).await;
    persist.pool().close().await;
    overlay
}

async fn query_overlay(persist: &PersistHandle) -> Result<LiveOverlay> {
    let settings = persist
        .fetch_settings()
        .await
        .context("could not read config.settings")?;
    let toggles = persist
        .fetch_feature_toggles()
        .await
        .context("could not read config.feature_toggles")?;
    Ok(LiveOverlay { settings, toggles })
}

fn is_secret_key(key: &str) -> bool {
    let leaf = key.rsplit('.').next().unwrap_or(key).to_lowercase();
    ["key", "secret", "token", "password"].iter().any(|marker| {
        leaf == *marker
            || leaf
                .strip_suffix(marker)
                .is_some_and(|rest| rest.ends_with('_'))
    })
}

fn setting_value(key: &str, value: &Value) -> toml::Value {
    if is_secret_key(key) {
        return toml::Value::String(REDACTED.to_owned());
    }
    toml::Value::try_from(value).unwrap_or_else(|_| toml::Value::String(value.to_string()))
}

fn toggle_value(enabled: bool, operator_set: bool) -> toml::Value {
    let mut table = toml::Table::new();
    table.insert("enabled".to_owned(), toml::Value::Boolean(enabled));
    table.insert(
        "operator_set".to_owned(),
        toml::Value::Boolean(operator_set),
    );
    toml::Value::Table(table)
}

fn overlay_table(overlay: &LiveOverlay) -> toml::Table {
    let mut settings = toml::Table::new();
    for (key, value) in &overlay.settings {
        settings.insert(key.clone(), setting_value(key, value));
    }
    let mut toggles = toml::Table::new();
    for (key, enabled, operator_set) in &overlay.toggles {
        toggles.insert(key.clone(), toggle_value(*enabled, *operator_set));
    }
    let mut root = toml::Table::new();
    root.insert("live_settings".to_owned(), toml::Value::Table(settings));
    root.insert(
        "live_feature_toggles".to_owned(),
        toml::Value::Table(toggles),
    );
    root
}

fn render_overlay(overlay: &LiveOverlay) -> String {
    let body = toml::to_string_pretty(&overlay_table(overlay))
        .unwrap_or_else(|e| format!("# the live overlay could not be rendered: {e}\n"));
    format!(
        "\n# LIVE OVERLAY: {} rows from config.settings and {} rows from config.feature_toggles.\n\
         # These override the boot values above at runtime. Secret-looking keys are redacted.\n{body}",
        overlay.settings.len(),
        overlay.toggles.len(),
    )
}

pub(super) fn render_report(boot_toml: &str, overlay: &OverlayState) -> String {
    let tail = match overlay {
        OverlayState::Live(live) => render_overlay(live),
        OverlayState::Unavailable(reason) => format!(
            "\n# LIVE OVERLAY: unavailable ({reason}). The values above are boot config only.\n"
        ),
    };
    format!("{BOOT_HEADER}{boot_toml}{tail}")
}

#[cfg(test)]
pub(crate) fn render_config(config: &ProxyConfig, shell: bool) -> Result<String> {
    if shell {
        Ok(load::to_shell_exports(config))
    } else {
        load::to_toml_string(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_overlay() -> LiveOverlay {
        LiveOverlay {
            settings: vec![
                ("composition_enabled".to_owned(), Value::Bool(true)),
                ("embedding.enabled".to_owned(), Value::Bool(true)),
                ("reminders.inject_lessons".to_owned(), Value::Bool(true)),
            ],
            toggles: vec![("reminders".to_owned(), true, true)],
        }
    }

    fn boot_toml() -> Result<String> {
        render_config(&ProxyConfig::default(), false)
    }

    #[test]
    fn render_default_config_as_toml() -> Result<()> {
        let out = render_config(&ProxyConfig::default(), false)?;
        assert!(out.contains("proxy_listen_addr"));
        assert!(out.contains("persist_enabled = true"));
        Ok(())
    }

    #[test]
    fn render_default_config_as_shell() -> Result<()> {
        let out = render_config(&ProxyConfig::default(), true)?;
        assert!(out.contains("MIKE_LISTEN_ADDR='127.0.0.1:9988'"));
        assert!(out.contains("MIKE_PERSIST_ENABLED='1'"));
        Ok(())
    }

    #[test]
    fn report_lists_live_overlay_values_beside_boot_config() -> Result<()> {
        let out = render_report(&boot_toml()?, &OverlayState::Live(sample_overlay()));
        assert!(
            out.contains("BOOT CONFIG"),
            "boot section unlabelled: {out}"
        );
        assert!(out.contains("LIVE OVERLAY"), "overlay unlabelled: {out}");
        let parsed: toml::Table = toml::from_str(&out)?;
        let settings = parsed
            .get("live_settings")
            .and_then(toml::Value::as_table)
            .context("live_settings table missing")?;
        assert_eq!(
            settings.get("embedding.enabled"),
            Some(&toml::Value::Boolean(true))
        );
        assert_eq!(
            settings.get("composition_enabled"),
            Some(&toml::Value::Boolean(true))
        );
        assert_eq!(
            settings.get("reminders.inject_lessons"),
            Some(&toml::Value::Boolean(true))
        );
        let toggle = parsed
            .get("live_feature_toggles")
            .and_then(toml::Value::as_table)
            .and_then(|t| t.get("reminders"))
            .and_then(toml::Value::as_table)
            .context("reminders toggle missing")?;
        assert_eq!(toggle.get("enabled"), Some(&toml::Value::Boolean(true)));
        assert_eq!(
            toggle.get("operator_set"),
            Some(&toml::Value::Boolean(true))
        );
        Ok(())
    }

    #[test]
    fn report_redacts_secret_looking_settings() -> Result<()> {
        let overlay = LiveOverlay {
            settings: vec![(
                "steerer.openrouter_api_key".to_owned(),
                Value::String("sk-or-v1-abcdef".to_owned()),
            )],
            toggles: Vec::new(),
        };
        let out = render_report(&boot_toml()?, &OverlayState::Live(overlay));
        assert!(!out.contains("sk-or-v1-abcdef"), "secret leaked: {out}");
        assert!(out.contains(REDACTED));
        Ok(())
    }

    #[test]
    fn report_labels_boot_only_output_when_overlay_unavailable() -> Result<()> {
        let out = render_report(
            &boot_toml()?,
            &OverlayState::Unavailable("connection refused".to_owned()),
        );
        assert!(out.contains("LIVE OVERLAY: unavailable (connection refused)"));
        assert!(out.contains("boot config only"));
        assert!(!out.contains("live_settings"));
        Ok(())
    }

    #[test]
    fn report_renders_values_toml_cannot_hold_as_json_strings() -> Result<()> {
        let overlay = LiveOverlay {
            settings: vec![("steerer.optional_model".to_owned(), Value::Null)],
            toggles: Vec::new(),
        };
        let out = render_report(&boot_toml()?, &OverlayState::Live(overlay));
        let parsed: toml::Table = toml::from_str(&out)?;
        let value = parsed
            .get("live_settings")
            .and_then(toml::Value::as_table)
            .and_then(|t| t.get("steerer.optional_model"));
        assert_eq!(value, Some(&toml::Value::String("null".to_owned())));
        Ok(())
    }

    #[test]
    fn secret_key_detection_matches_key_secret_token_and_password_names() {
        assert!(is_secret_key("steerer.openrouter_api_key"));
        assert!(is_secret_key("integrations.github_TOKEN"));
        assert!(is_secret_key("db.password"));
        assert!(is_secret_key("app.client_secret"));
        assert!(!is_secret_key("embedding.enabled"));
        assert!(!is_secret_key("steerer.max_tokens"));
    }
}
