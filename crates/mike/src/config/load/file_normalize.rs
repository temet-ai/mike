use anyhow::{Result, ensure};

use crate::environment::Environment;

const METADATA: &[&str] = &[
    "setup",
    "setup_features",
    "claude_route",
    "claude_function",
    "claude_mods",
];

pub(super) fn normalize(table: &toml::Table, environment: &dyn Environment) -> Result<toml::Table> {
    let mut defaults = serde_json::to_value(crate::config::ProxyConfig::default())?;
    defaults["steerer"]["openrouter_api_key"] = serde_json::Value::Null;
    let mut table = expand_fields(table)?;
    let key = table
        .remove("steerer_openrouter_api_key")
        .filter(|value| !blank(value));
    let reference = table.remove("steerer_openrouter_api_key_from_env");
    let reference = reference
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("configuration environment references must be text"))
        })
        .transpose()?;
    let value = key.or_else(|| {
        reference
            .and_then(|name| environment.var(&name))
            .filter(|value| !value.trim().is_empty())
            .map(toml::Value::String)
    });
    if let Some(value) = value {
        let steerer = table
            .entry("steerer")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        steerer
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("invalid steerer configuration"))?
            .insert("openrouter_api_key".into(), value);
    }
    normalize_table(&table, &defaults, true, environment)
}

fn blank(value: &toml::Value) -> bool {
    value.as_str().is_some_and(|text| text.trim().is_empty())
}

fn expand_fields(table: &toml::Table) -> Result<toml::Table> {
    let mut expanded = toml::Table::new();
    for (name, value) in table {
        let path: Vec<String> = name
            .to_ascii_lowercase()
            .split("__")
            .map(str::to_string)
            .collect();
        super::import::insert(&mut expanded, &path, value.clone())?;
    }
    Ok(expanded)
}

fn normalize_table(
    table: &toml::Table,
    defaults: &serde_json::Value,
    root: bool,
    environment: &dyn Environment,
) -> Result<toml::Table> {
    let mut normalized = toml::Table::new();
    for (name, value) in table {
        let name = name.to_ascii_lowercase();
        if root && METADATA.contains(&name.as_str()) {
            ensure!(value.is_str(), "onboarding metadata must be text");
            continue;
        }
        let (base, from_env) = name
            .strip_suffix("_from_env")
            .map_or((name.as_str(), false), |base| (base, true));
        let key = match base {
            "listen_addr" if root => "proxy_listen_addr",
            "inspect_addr" if root => "inspect_listen_addr",
            key => key,
        };
        let default = defaults.get(key).ok_or_else(|| {
            anyhow::anyhow!("selected configuration contains an unsupported boot setting")
        })?;
        if blank(value) {
            continue;
        }
        if from_env {
            if table.get(base).is_some_and(|value| !blank(value)) {
                continue;
            }
            let variable = value.as_str().ok_or_else(|| {
                anyhow::anyhow!("configuration environment references must be text")
            })?;
            if let Some(value) = environment.var(variable) {
                if value.trim().is_empty() {
                    continue;
                }
                normalized.insert(
                    key.into(),
                    normalize_value(&toml::Value::String(value), default, key, environment)?,
                );
            }
        } else {
            normalized.insert(
                key.into(),
                normalize_value(value, default, key, environment)?,
            );
        }
    }
    Ok(normalized)
}

fn normalize_value(
    value: &toml::Value,
    default: &serde_json::Value,
    key: &str,
    environment: &dyn Environment,
) -> Result<toml::Value> {
    if let toml::Value::Table(table) = value {
        return Ok(toml::Value::Table(normalize_table(
            table,
            default,
            false,
            environment,
        )?));
    }
    let Some(text) = value.as_str() else {
        return Ok(value.clone());
    };
    let invalid =
        || anyhow::anyhow!("invalid boot configuration field type; values omitted for privacy");
    if default.is_boolean() {
        return crate::config::parse_permissive_bool_str(text)
            .map(toml::Value::Boolean)
            .map_err(|_| invalid());
    }
    if default.is_i64() || default.is_u64() {
        return text
            .parse()
            .map(toml::Value::Integer)
            .map_err(|_| invalid());
    }
    if default.is_f64() {
        return text.parse().map(toml::Value::Float).map_err(|_| invalid());
    }
    if default.is_array() {
        let strings = if key == "launch_claude_args" {
            crate::config::launch::parse_claude_args_env(text)
        } else {
            text.split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect()
        };
        return Ok(toml::Value::Array(
            strings.into_iter().map(toml::Value::String).collect(),
        ));
    }
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::MapEnvironment;

    #[test]
    fn named_environment_variable_supplies_the_value_from_the_injected_environment() -> Result<()> {
        let environment = MapEnvironment::new(&[("MY_KEY", "sk-from-map")]);
        let table = normalize(
            &toml::from_str("steerer_openrouter_api_key_from_env='MY_KEY'")?,
            &environment,
        )?;
        assert_eq!(
            table["steerer"]["openrouter_api_key"].as_str(),
            Some("sk-from-map")
        );
        Ok(())
    }

    #[test]
    fn named_environment_variable_missing_from_the_injected_environment_supplies_nothing()
    -> Result<()> {
        let table = normalize(
            &toml::from_str("steerer_openrouter_api_key_from_env='MY_KEY'")?,
            &MapEnvironment::default(),
        )?;
        assert!(!table.contains_key("steerer"));
        Ok(())
    }

    #[test]
    fn onboarding_config_keeps_boot_values_and_excludes_client_metadata() -> Result<()> {
        let table = normalize(
            &toml::from_str(
                "setup='unfinished'\nclaude_route='plain'\nlisten_addr='127.0.0.1:7100'\npersist_enabled='0'\nmax_requests='42'\n",
            )?,
            &MapEnvironment::default(),
        )?;
        assert_eq!(table["proxy_listen_addr"].as_str(), Some("127.0.0.1:7100"));
        assert_eq!(table["persist_enabled"].as_bool(), Some(false));
        assert_eq!(table["max_requests"].as_integer(), Some(42));
        assert!(!table.contains_key("setup"));
        assert!(!table.contains_key("claude_route"));
        Ok(())
    }

    #[test]
    fn legacy_empty_values_and_nested_keys_preserve_typed_arrays() -> Result<()> {
        let table = normalize(
            &toml::from_str(
                "database_url=' '\nmax_requests=''\nlaunch_claude_args=['--chrome','--verbose']\nsteerer__model='fixture'\n",
            )?,
            &MapEnvironment::default(),
        )?;
        assert!(!table.contains_key("database_url"));
        assert!(!table.contains_key("max_requests"));
        assert_eq!(table["steerer"]["model"].as_str(), Some("fixture"));
        assert_eq!(table["launch_claude_args"][0].as_str(), Some("--chrome"));
        assert_eq!(table["launch_claude_args"][1].as_str(), Some("--verbose"));
        Ok(())
    }

    #[test]
    fn unknown_settings_and_invalid_metadata_are_refused_without_values() -> Result<()> {
        for input in [
            "unknown='secret'",
            "setup=['secret']",
            "max_requests='secret'",
        ] {
            let error = normalize(&toml::from_str(input)?, &MapEnvironment::default())
                .err()
                .ok_or_else(|| anyhow::anyhow!("invalid config accepted"))?;
            assert!(!error.to_string().contains("secret"));
        }
        Ok(())
    }
}
