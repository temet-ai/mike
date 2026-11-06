use serde::Serialize;

use super::types::DossierInputs;
use crate::eval::bundles::{canonical_json, sha256_hex};

pub fn inputs_digest(inputs: &DossierInputs) -> String {
    let value = serde_json::to_value(inputs).unwrap_or(serde_json::Value::Null);
    sha256_hex(canonical_json(&value).to_string().as_bytes())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SourceVersions {
    pub classifier: Vec<String>,
    pub builder: Vec<String>,
    pub extractor: Vec<String>,
    pub mode: String,
}

fn distinct(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = values.collect();
    out.sort();
    out.dedup();
    out
}

pub fn source_versions(inputs: &DossierInputs) -> SourceVersions {
    SourceVersions {
        classifier: distinct(
            inputs
                .stream
                .iter()
                .filter_map(|s| s.classifier_version.clone()),
        ),
        builder: distinct(
            inputs
                .turns
                .iter()
                .filter_map(|t| t.builder_version.clone()),
        ),
        extractor: distinct(inputs.entries.iter().map(|e| e.extractor_version.clone())),
        mode: inputs.as_of.mode.code().to_owned(),
    }
}
