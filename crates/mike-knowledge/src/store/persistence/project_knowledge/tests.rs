use super::documents::DocumentInput;
use serde_json::json;
use sha2::{Digest, Sha256};

pub fn document(body: &str, status: &str) -> DocumentInput {
    DocumentInput {
        relative_path: "design.md".into(),
        title: "Notebook design".into(),
        content_hash: Sha256::digest(body.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        body: body.into(),
        kind: "design".into(),
        status: status.into(),
        metadata: json!({}),
    }
}
