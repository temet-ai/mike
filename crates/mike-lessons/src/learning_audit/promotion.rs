use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};

use super::rows::{non_empty, truthy};

const LEDGER_FILE: &str = "skill-installs.json";
const MANIFEST_FILE: &str = "manifest.json";
const RECEIPT_FILE: &str = "latest.json";
const SKILL_FILE_NAME: &str = "SKILL.md";

#[derive(Debug, Default)]
pub(super) struct PromotionState {
    pub(super) ledger: Map<String, Value>,
    pub(super) manifest_names: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Publication {
    pub(super) state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) lessons: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) memory_projects: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) skill_installations: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) conflicts: Option<Value>,
}

impl Default for Publication {
    fn default() -> Self {
        Self {
            state: "missing".to_owned(),
            updated_at: None,
            lessons: None,
            memory_projects: None,
            skill_installations: None,
            conflicts: None,
        }
    }
}

fn read_json(path: &Path) -> Result<Option<Value>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .with_context(|| format!("{} is not valid JSON", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn manifest_skill_names(manifest: &Value) -> BTreeSet<String> {
    manifest
        .as_object()
        .into_iter()
        .flat_map(Map::keys)
        .map(Path::new)
        .filter(|file| file.file_name().is_some_and(|name| name == SKILL_FILE_NAME))
        .filter_map(|file| file.parent()?.file_name()?.to_str().map(str::to_owned))
        .collect()
}

impl PromotionState {
    pub(super) fn load(directory: &Path) -> Result<Self> {
        let ledger = match read_json(&directory.join(LEDGER_FILE))? {
            Some(Value::Object(fields)) => fields,
            _ => Map::new(),
        };
        let manifest_names = read_json(&directory.join(MANIFEST_FILE))?
            .map(|manifest| manifest_skill_names(&manifest))
            .unwrap_or_default();
        Ok(Self {
            ledger,
            manifest_names,
        })
    }

    pub(super) fn ledger_folder_names(&self) -> BTreeSet<&str> {
        self.ledger
            .keys()
            .filter_map(|key| key.split_once(':').map(|(_, folder)| folder))
            .collect()
    }

    pub(super) fn live_skills_per_project(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for (key, entry) in &self.ledger {
            if !truthy(entry.get("retired_at")) {
                let project = key.split(':').next().unwrap_or(key);
                *counts.entry(project.to_owned()).or_default() += 1;
            }
        }
        counts.into_iter().collect()
    }
}

pub(super) fn publication_status(directory: &Path) -> Result<Publication> {
    let path = directory.join(RECEIPT_FILE);
    let Some(receipt) = read_json(&path)? else {
        return Ok(Publication::default());
    };
    let updated_at = std::fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .with_context(|| format!("cannot read the modification time of {}", path.display()))?;
    let conflicts = receipt.get("conflicts").cloned().unwrap_or(Value::Null);
    let state = if truthy(Some(&conflicts)) {
        "conflict"
    } else {
        "published"
    };
    Ok(Publication {
        state: state.to_owned(),
        updated_at: Some(DateTime::<Utc>::from(updated_at).to_rfc3339()),
        lessons: receipt.get("lessons").cloned(),
        memory_projects: receipt.get("memory_projects").cloned(),
        skill_installations: Some(
            receipt
                .get("skills")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        ),
        conflicts: Some(conflicts),
    })
}

pub(super) fn ledger_project_of(row: &Value) -> Option<&str> {
    non_empty(row, "project_uuid")
}
