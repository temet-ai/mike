use crate::analysis::embeddings::prefixes_for;
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::ModelKeys;
use crate::store::persistence::{PersistError, PersistHandle};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const LEASE_ROLE: &str = "knowledge_embedding";

fn hex_digest(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn key_with_version(model: &str, chunk_max_chars: usize, prefix_version: &str) -> String {
    hex_digest(&format!(
        "knowledge-v2:{model}:{chunk_max_chars}:{prefix_version}"
    ))
}

pub fn key_for(model: &str, chunk_max_chars: usize) -> String {
    key_with_version(model, chunk_max_chars, prefixes_for(model).version)
}

pub fn previous_key_for(model: &str, chunk_max_chars: usize) -> Option<String> {
    prefixes_for(model)
        .previous_version
        .map(|version| key_with_version(model, chunk_max_chars, version))
}

pub fn keys_for(model: &str, chunk_max_chars: usize) -> ModelKeys {
    ModelKeys::new(
        key_for(model, chunk_max_chars),
        previous_key_for(model, chunk_max_chars),
    )
}

pub fn model_keys(service: &EmbeddingService) -> ModelKeys {
    keys_for(&service.model_name(), service.chunk_max_chars())
}

#[cfg(test)]
pub fn unversioned_v2_key(model: &str, chunk_max_chars: usize) -> String {
    hex_digest(&format!("knowledge-v2:{model}:{chunk_max_chars}"))
}

pub fn model_key(service: &EmbeddingService) -> String {
    key_for(&service.model_name(), service.chunk_max_chars())
}

fn url_v1_key(url: &str, model: &str, chunk_max_chars: usize) -> String {
    hex_digest(&format!("knowledge-v1:{url}:{model}:{chunk_max_chars}"))
}

fn candidate_urls(stored: Vec<String>, service: &EmbeddingService) -> BTreeSet<String> {
    stored
        .into_iter()
        .chain([service.url(), EmbeddingConfig::default().url])
        .flat_map(|url| [url.trim_end_matches('/').to_owned(), url])
        .collect()
}

pub fn url_v1_keys(stored_urls: Vec<String>, service: &EmbeddingService) -> Vec<String> {
    let model = service.model_name();
    let chunk = service.chunk_max_chars();
    candidate_urls(stored_urls, service)
        .iter()
        .map(|url| url_v1_key(url, &model, chunk))
        .collect()
}

pub async fn rekey_url_keyed_rows(
    persist: &PersistHandle,
    service: &EmbeddingService,
) -> Result<u64, PersistError> {
    let stored = persist
        .knowledge()
        .knowledge_embedding_url_candidates()
        .await?;
    let keys = model_keys(service).accepted();
    let target = keys.last().cloned().unwrap_or_else(|| model_key(service));
    persist
        .knowledge()
        .rekey_knowledge_embeddings(&url_v1_keys(stored, service), &target)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(url: &str, model: &str, chunk: usize) -> EmbeddingService {
        EmbeddingService::new(&EmbeddingConfig {
            enabled: true,
            url: url.into(),
            model: model.into(),
            chunk_max_chars: chunk,
        })
    }

    #[test]
    fn key_ignores_the_serving_host() {
        let loopback = service("http://127.0.0.1:11434/v1", "nomic-embed-text-v1.5", 1024);
        let lan = service(
            "http://192.168.1.20:11434/v1",
            "nomic-embed-text-v1.5",
            1024,
        );
        assert_eq!(model_key(&loopback), model_key(&lan));
        assert_eq!(model_key(&lan), key_for("nomic-embed-text-v1.5", 1024));
    }

    #[test]
    fn key_changes_with_model_and_chunking() {
        let base = key_for("nomic-embed-text-v1.5", 1024);
        assert_ne!(base, key_for("other-model", 1024));
        assert_ne!(base, key_for("nomic-embed-text-v1.5", 512));
    }

    #[test]
    fn key_includes_the_prefix_version_of_the_model() {
        assert_eq!(
            key_for("embeddinggemma", 1024),
            hex_digest("knowledge-v2:embeddinggemma:1024:embeddinggemma-v2")
        );
        assert_ne!(
            key_for("embeddinggemma", 1024),
            unversioned_v2_key("embeddinggemma", 1024)
        );
        assert_ne!(
            key_for("nomic-embed-text-v1.5", 1024),
            unversioned_v2_key("nomic-embed-text-v1.5", 1024)
        );
    }

    #[test]
    fn url_v1_keys_reproduce_the_hashes_stored_before_the_change() {
        let svc = service("http://127.0.0.1:11434/v1/", "nomic-embed-text-v1.5", 1024);
        let keys = url_v1_keys(vec!["http://192.168.1.20:11434/v1".into()], &svc);
        assert!(keys.iter().any(|k| k.starts_with("4f945851")));
        assert!(keys.iter().any(|k| k.starts_with("964d9ea7")));
        assert!(!keys.contains(&model_key(&svc)));
    }
}
