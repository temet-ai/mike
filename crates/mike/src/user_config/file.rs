use std::path::{Path, PathBuf};
use std::str::FromStr;

use toml_edit::{DocumentMut, Item, Table, Value};

use super::UserConfigError;

const FROM_ENV_SUFFIX: &str = "_from_env";
const SPACE_SEPARATED_LISTS: [&str; 1] = ["launch_claude_args"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Setting {
    Literal(String),
    FromEnv(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) env_name: String,
    pub(crate) setting: Setting,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SettingValue<'a> {
    Text(&'a str),
    Bool(bool),
}

pub(crate) struct ConfigFile {
    path: PathBuf,
    doc: DocumentMut,
}

impl ConfigFile {
    pub(crate) fn open(path: &Path) -> Result<Self, UserConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(source) => {
                return Err(UserConfigError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let doc = DocumentMut::from_str(&text).map_err(|e| UserConfigError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        Ok(Self {
            path: path.to_path_buf(),
            doc,
        })
    }

    pub(crate) fn entries(&self) -> Result<Vec<Entry>, UserConfigError> {
        let mut out = Vec::new();
        self.flatten(self.doc.as_table(), "", &mut out)?;
        Ok(out)
    }

    fn flatten(
        &self,
        table: &Table,
        prefix: &str,
        out: &mut Vec<Entry>,
    ) -> Result<(), UserConfigError> {
        for (raw_key, item) in table.iter() {
            let key = self.checked_key(raw_key)?;
            let name = if prefix.is_empty() {
                key
            } else {
                format!("{prefix}__{key}")
            };
            match item {
                Item::Table(nested) => self.flatten(nested, &name, out)?,
                Item::Value(Value::InlineTable(nested)) => {
                    self.flatten(&nested.clone().into_table(), &name, out)?;
                }
                Item::Value(value) => out.push(self.entry(&name, value)?),
                Item::ArrayOfTables(_) | Item::None => {
                    return Err(self.unsupported(&name, "table array"));
                }
            }
        }
        Ok(())
    }

    fn checked_key(&self, raw: &str) -> Result<String, UserConfigError> {
        let key = raw.to_ascii_lowercase();
        let bad = |reason: &str| UserConfigError::BadKey {
            path: self.path.clone(),
            key: raw.to_string(),
            reason: reason.to_string(),
        };
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(bad("only letters, digits and underscores are allowed"));
        }
        if key.starts_with("mike_") {
            return Err(bad("drop the leading `mike_`"));
        }
        Ok(key)
    }

    fn entry(&self, name: &str, value: &Value) -> Result<Entry, UserConfigError> {
        let text = self.value_text(name, value)?;
        let (base, setting) = match name.strip_suffix(FROM_ENV_SUFFIX) {
            Some(base) if !base.is_empty() => (base, Setting::FromEnv(text)),
            _ => (name, Setting::Literal(text)),
        };
        Ok(Entry {
            env_name: env_name_for(base),
            setting,
        })
    }

    fn value_text(&self, name: &str, value: &Value) -> Result<String, UserConfigError> {
        if let Some(text) = scalar_text(value) {
            return Ok(text);
        }
        let Value::Array(items) = value else {
            return Err(self.unsupported(name, value.type_name()));
        };
        let parts: Option<Vec<String>> = items.iter().map(scalar_text).collect();
        let separator = if SPACE_SEPARATED_LISTS.contains(&name) {
            " "
        } else {
            ","
        };
        parts
            .map(|p| p.join(separator))
            .ok_or_else(|| self.unsupported(name, "array of tables or dates"))
    }

    fn unsupported(&self, name: &str, kind: &'static str) -> UserConfigError {
        UserConfigError::UnsupportedValue {
            path: self.path.clone(),
            key: name.to_string(),
            kind,
        }
    }

    pub(crate) fn literal(&self, env_name: &str) -> Result<Option<String>, UserConfigError> {
        Ok(self
            .entries()?
            .into_iter()
            .find(|entry| entry.env_name == env_name)
            .and_then(|entry| match entry.setting {
                Setting::Literal(value) => Some(value),
                Setting::FromEnv(_) => None,
            }))
    }

    pub(crate) fn contains_env(&self, env_name: &str) -> Result<bool, UserConfigError> {
        Ok(self.entries()?.iter().any(|e| e.env_name == env_name))
    }

    pub(crate) fn set(
        &mut self,
        key: &str,
        value: SettingValue<'_>,
    ) -> Result<(), UserConfigError> {
        let key = self.checked_key(&file_key_for(key))?;
        let item = match value {
            SettingValue::Text(text) => toml_edit::value(text),
            SettingValue::Bool(flag) => toml_edit::value(flag),
        };
        self.doc.as_table_mut().insert(&key, item);
        Ok(())
    }

    pub(crate) fn remove(&mut self, key: &str) {
        self.doc.as_table_mut().remove(&file_key_for(key));
    }

    pub(crate) fn save(&self) -> Result<(), UserConfigError> {
        let write_err = |source| UserConfigError::Write {
            path: self.path.clone(),
            source,
        };
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            create_private_dir(dir).map_err(write_err)?;
        }
        crate::owner_only_file::write(&self.path, &self.doc.to_string()).map_err(write_err)
    }
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.value().clone()),
        Value::Integer(i) => Some(i.value().to_string()),
        Value::Float(f) => Some(f.value().to_string()),
        Value::Boolean(b) => Some(b.value().to_string()),
        _ => None,
    }
}

pub(crate) fn env_name_for(key: &str) -> String {
    let key = match key {
        "proxy_listen_addr" => "listen_addr",
        "inspect_listen_addr" => "inspect_addr",
        other => other,
    };
    format!("MIKE_{}", key.to_ascii_uppercase())
}

pub(crate) fn file_key_for(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_prefix("mike_").unwrap_or(&lower).to_string()
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}
