use std::path::Path;

use super::UserConfigError;
use super::file::{ConfigFile, SettingValue, env_name_for, file_key_for};

const PREFIX: &str = "MIKE_";

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportReport {
    pub(crate) imported: Vec<String>,
    pub(crate) kept: Vec<String>,
    pub(crate) ignored: usize,
}

pub(crate) fn parse_dotenv(text: &str) -> Vec<(String, String)> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").map_or(line, str::trim_start);
    let (name, raw) = line.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Some((name.to_string(), unquote(raw.trim())))
}

fn unquote(raw: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = raw
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.to_string();
        }
    }
    match raw.find(" #") {
        Some(at) => raw[..at].trim_end().to_string(),
        None => raw.to_string(),
    }
}

pub(crate) fn import_env(
    env_text: &str,
    config_path: &Path,
) -> Result<ImportReport, UserConfigError> {
    let mut file = ConfigFile::open(config_path)?;
    let mut report = ImportReport::default();
    for (name, value) in parse_dotenv(env_text) {
        if !name.starts_with(PREFIX) {
            report.ignored += 1;
            continue;
        }
        let key = file_key_for(&name);
        if file.contains_env(&env_name_for(&key))? {
            report.kept.push(key);
        } else {
            file.set(&key, SettingValue::Text(&value))?;
            report.imported.push(key);
        }
    }
    if !report.imported.is_empty() {
        file.save()?;
    }
    Ok(report)
}
