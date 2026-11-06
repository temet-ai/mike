use std::path::{Path, PathBuf};

pub(crate) fn persist_boot_setting_at(path: &Path, key: &str, value: &str) -> anyhow::Result<()> {
    let updates =
        toml::Table::from_iter([(key.to_string(), toml::Value::String(value.to_string()))]);
    update_selected(path, updates)?;
    Ok(())
}

fn update_selected(path: &Path, updates: toml::Table) -> anyhow::Result<Option<PathBuf>> {
    if !path.is_absolute() {
        anyhow::bail!(
            "setup needs an absolute selected configuration path; restart mike and retry"
        );
    }
    crate::config::write::update_selected(path, updates)
}

pub(crate) fn save_provider_key_at(path: &Path, value: &str) -> anyhow::Result<()> {
    save_provider_setting_at(path, value, false)
}

pub(crate) fn save_provider_setting_at(
    path: &Path,
    value: &str,
    from_env: bool,
) -> anyhow::Result<()> {
    crate::config::write::edit_selected(path, |table| {
        table.remove("steerer_openrouter_api_key");
        table.remove("steerer_openrouter_api_key_from_env");
        table.remove("steerer__openrouter_api_key");
        table.remove("steerer__openrouter_api_key_from_env");
        let steerer = table
            .entry("steerer")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let Some(steerer) = steerer.as_table_mut() {
            steerer.remove("openrouter_api_key");
            steerer.remove("openrouter_api_key_from_env");
            let name = if from_env {
                "openrouter_api_key_from_env"
            } else {
                "openrouter_api_key"
            };
            steerer.insert(name.into(), toml::Value::String(value.into()));
        }
    })?;
    Ok(())
}

pub(crate) fn save_database_mode_at(
    path: &Path,
    database_url: Option<&str>,
) -> anyhow::Result<Option<PathBuf>> {
    let mut updates = toml::Table::from_iter([(
        "persist_enabled".to_string(),
        toml::Value::Boolean(database_url.is_some()),
    )]);
    if let Some(url) = database_url {
        updates.insert(
            "db_fallback_enabled".to_string(),
            toml::Value::Boolean(false),
        );
        updates.insert(
            "database_url".to_string(),
            toml::Value::String(url.to_string()),
        );
    }
    update_selected(path, updates)
}

#[cfg(test)]
#[path = "persistence_contract_tests.rs"]
mod tests;
