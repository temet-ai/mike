use anyhow::{Context, Result};

use crate::config::ProxyConfig;

pub fn convert(
    assignments: Vec<(String, String)>,
    ignore_unknown: bool,
) -> Result<(toml::Table, usize)> {
    let defaults = serde_json::to_value(ProxyConfig::default())?;
    let mut table = toml::Table::new();
    let mut skipped = 0;
    for (key, value) in assignments {
        let Some(path) = field_path(&key, &defaults) else {
            anyhow::ensure!(
                ignore_unknown,
                "unsupported dotenv variable {key}; use --ignore-unknown to skip unrelated variables"
            );
            skipped += 1;
            continue;
        };
        let typed = typed_value(&path, &value, &defaults).map_err(|_| {
            anyhow::anyhow!("invalid dotenv value for {key}; contents omitted for privacy")
        })?;
        insert(&mut table, &path, typed)?;
    }
    super::file::validate(&table)?;
    Ok((table, skipped))
}

fn field_path(key: &str, defaults: &serde_json::Value) -> Option<Vec<String>> {
    if matches!(
        key,
        "MIKE_STEERER_OPENROUTER_API_KEY"
            | "MIKE_STEERER__OPENROUTER_API_KEY"
            | "OPENROUTER_API_KEY"
    ) {
        return Some(vec!["steerer".to_owned(), "openrouter_api_key".to_owned()]);
    }
    let name = key.strip_prefix("MIKE_")?.to_ascii_lowercase();
    let name = match name.as_str() {
        "listen_addr" => "proxy_listen_addr",
        "inspect_addr" => "inspect_listen_addr",
        other => other,
    };
    let path: Vec<String> = name.split("__").map(str::to_owned).collect();
    let mut current = defaults;
    for part in &path {
        current = current.get(part)?;
    }
    Some(path)
}

fn typed_value(path: &[String], value: &str, defaults: &serde_json::Value) -> Result<toml::Value> {
    if path == ["launch_claude_args"] {
        return Ok(toml::Value::Array(
            crate::config::launch::parse_claude_args_env(value)
                .into_iter()
                .map(toml::Value::String)
                .collect(),
        ));
    }
    let exemplar = path
        .iter()
        .try_fold(defaults, |current, part| current.get(part));
    match exemplar {
        Some(serde_json::Value::Bool(_)) => Ok(toml::Value::Boolean(
            crate::config::parse_permissive_bool_str(value).map_err(anyhow::Error::msg)?,
        )),
        Some(serde_json::Value::Number(n)) if n.is_f64() => Ok(toml::Value::Float(value.parse()?)),
        Some(serde_json::Value::Number(_)) => Ok(toml::Value::Integer(value.parse()?)),
        Some(serde_json::Value::Array(_)) => Ok(toml::Value::Array(
            value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| toml::Value::String(s.to_owned()))
                .collect(),
        )),
        Some(serde_json::Value::Object(_)) => {
            anyhow::bail!("nested fields require double underscores")
        }
        _ => Ok(toml::Value::String(value.to_owned())),
    }
}

pub fn insert(table: &mut toml::Table, path: &[String], value: toml::Value) -> Result<()> {
    let (first, rest) = path.split_first().context("empty field path")?;
    if rest.is_empty() {
        table.insert(first.clone(), value);
        return Ok(());
    }
    let nested = table
        .entry(first)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    insert(
        nested.as_table_mut().context("conflicting field paths")?,
        rest,
        value,
    )
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn import_maps_legacy_names_and_keeps_secret_literals() -> Result<()> {
        let (table, skipped) = convert(
            vec![
                ("MIKE_LISTEN_ADDR".into(), "127.0.0.1:7777".into()),
                ("MIKE_PERSIST_ENABLED".into(), "0".into()),
                ("MIKE_STEERER_OPENROUTER_API_KEY".into(), "s $`#λ".into()),
            ],
            false,
        )?;
        assert_eq!(skipped, 0);
        assert_eq!(table["proxy_listen_addr"].as_str(), Some("127.0.0.1:7777"));
        assert_eq!(table["persist_enabled"].as_bool(), Some(false));
        assert_eq!(
            table["steerer"]["openrouter_api_key"].as_str(),
            Some("s $`#λ")
        );
        Ok(())
    }

    #[test]
    fn import_rejects_unknown_or_bad_types_without_values() -> Result<()> {
        let bad = convert(
            vec![("MIKE_MAX_REQUESTS".into(), "planted-secret".into())],
            false,
        )
        .err()
        .context("bad type was accepted")?;
        assert!(!bad.to_string().contains("planted-secret"));
        assert!(convert(vec![("UNKNOWN".into(), "secret".into())], false).is_err());
        Ok(())
    }
}
