use std::time::Duration;

use serde::Deserialize;

use crate::analysis::embeddings::{EmbeddingConfig, EmbeddingService};
use crate::db_provision::{Call, Runner, SystemRunner};
use crate::finding::Finding;
use crate::tui::client::{ApiClient, CapabilityState};

pub(super) const OLLAMA_BASE: &str = "http://127.0.0.1:11434";
pub(super) const OLLAMA_MODEL: &str = "embeddinggemma";
pub(super) const VECTOR_WIDTH: usize = 768;
const PROBE_WAIT: Duration = Duration::from_secs(3);
const PULL_WAIT: Duration = Duration::from_secs(1800);
const START_WAIT: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
struct Tags {
    models: Vec<TagModel>,
}

#[derive(Debug, Deserialize)]
struct TagModel {
    name: String,
}

pub(super) async fn ollama_models(base: &str) -> Option<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_WAIT)
        .build()
        .ok()?;
    let response = client.get(format!("{base}/api/tags")).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let tags: Tags = response.json().await.ok()?;
    Some(tags.models.into_iter().map(|m| m.name).collect())
}

pub(super) fn has_model(models: &[String], model: &str) -> bool {
    models.iter().any(|name| {
        name == model
            || name
                .strip_prefix(model)
                .is_some_and(|rest| rest.starts_with(':'))
    })
}

pub(super) async fn embed_once(url: &str, model: &str) -> Result<usize, String> {
    let service = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: url.to_string(),
        model: model.to_string(),
        ..EmbeddingConfig::default()
    });
    service
        .embed_query("ping")
        .await
        .map(|vector| vector.len())
        .map_err(|e| e.to_string())
}

pub(super) async fn fetch_one(url: &str, model: &str) -> Finding {
    match embed_once(url, model).await {
        Ok(width) if width == VECTOR_WIDTH => Finding::ok(format!(
            "Fetched one embedding of {width} numbers from {url} with the model {model}."
        )),
        Ok(width) => Finding::failed(format!(
            "{url} returned {width} numbers for one text, but mike stores vectors of {VECTOR_WIDTH}. Choose a model that returns {VECTOR_WIDTH}."
        )),
        Err(why) => Finding::failed(format!("Fetching one embedding from {url} failed: {why}")),
    }
}

pub(super) async fn pull_model() -> Finding {
    let call = Call::new("ollama", &["pull", OLLAMA_MODEL]);
    match tokio::time::timeout(PULL_WAIT, SystemRunner.run(&call)).await {
        Err(_) => Finding::failed(format!(
            "`{}` did not finish within {} minutes.",
            call.shown(),
            PULL_WAIT.as_secs() / 60
        )),
        Ok(Err(e)) => Finding::failed(format!("`{}` could not run: {e}", call.shown())),
        Ok(Ok(out)) if out.success => Finding::ok(format!("`{}` finished.", call.shown())),
        Ok(Ok(out)) => Finding::failed(format!("`{}` failed: {}", call.shown(), out.detail())),
    }
}

pub(super) async fn start_server(base: &str, brew: bool) -> Option<Vec<String>> {
    if brew {
        let call = Call::new("brew", &["services", "start", "ollama"]);
        if SystemRunner.run(&call).await.is_ok_and(|out| out.success) {
            let started = tokio::time::Instant::now();
            while started.elapsed() < START_WAIT {
                if let Some(models) = ollama_models(base).await {
                    return Some(models);
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
    None
}

pub(super) async fn send_settings(
    client: &ApiClient,
    url: &str,
    model: Option<&str>,
) -> Vec<Finding> {
    let mut failures = Vec::new();
    let mut writes: Vec<(&str, serde_json::Value)> =
        vec![("embedding.url", serde_json::json!(url))];
    if let Some(model) = model {
        writes.push(("embedding.model", serde_json::json!(model)));
    }
    writes.push(("embedding.enabled", serde_json::json!(true)));
    for (key, value) in &writes {
        if let Err(why) = client.patch_setting(key, value.clone()).await {
            failures.push(Finding::failed(format!("The proxy refused {key}: {why}")));
        }
    }
    if failures.is_empty() {
        vec![Finding::ok(format!(
            "The running proxy accepted {} embedding settings.",
            writes.len()
        ))]
    } else {
        failures
    }
}

pub(super) async fn recall_row(client: &ApiClient) -> Finding {
    let Some(payload) = client.capabilities().await else {
        return Finding::failed("The proxy did not return its capability rows.");
    };
    match payload
        .capabilities
        .iter()
        .find(|r| r.id == "semantic_recall")
    {
        None => Finding::note("The proxy lists no search-by-meaning row."),
        Some(row) => match row.state {
            CapabilityState::Active => Finding::ok(format!("{}: active", row.label)),
            CapabilityState::Off => Finding::note(format!("{}: off", row.label)),
            CapabilityState::Degraded => Finding::failed(format!(
                "{}: degraded. {} {}",
                row.label, row.detail, row.fix
            )),
            CapabilityState::Inert => {
                Finding::failed(format!("{}: inert. {} {}", row.label, row.detail, row.fix))
            }
        },
    }
}
