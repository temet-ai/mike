use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::finding::Finding;
use crate::tui::client::ApiClient;

pub(super) const LOCAL_BASE: &str = "http://127.0.0.1:11435";
pub(super) const MODEL_NAME: &str = "bge-reranker-v2-m3";
const RERANK_WAIT: Duration = Duration::from_secs(150);
const QUERY: &str = "How do I restart the proxy after changing its code?";
const DOCUMENTS: [&str; 3] = [
    "Tomatoes grow best in full sun with regular watering.",
    "Run ./scripts/start-proxy.sh to rebuild and restart the proxy after changing crates/mike.",
    "The capital of France is Paris.",
];
const ANSWER: usize = 1;

#[derive(Debug, Deserialize)]
struct Ranked {
    results: Vec<Scored>,
}

#[derive(Debug, Deserialize)]
struct Scored {
    index: usize,
    relevance_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Reply {
    pub(super) count: usize,
    pub(super) top: Option<usize>,
    pub(super) elapsed: Duration,
}

pub(super) fn trimmed(base: &str) -> &str {
    base.trim().trim_end_matches('/')
}

pub(super) async fn rerank_once(base: &str) -> Result<Reply, String> {
    let client = reqwest::Client::builder()
        .timeout(RERANK_WAIT)
        .build()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let response = client
        .post(format!("{}/v1/rerank", trimmed(base)))
        .json(&serde_json::json!({"model": MODEL_NAME, "query": QUERY, "documents": DOCUMENTS}))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", body.trim()));
    }
    let ranked: Ranked = response.json().await.map_err(|e| e.to_string())?;
    let top = ranked
        .results
        .iter()
        .max_by(|a, b| a.relevance_score.total_cmp(&b.relevance_score))
        .map(|s| s.index);
    Ok(Reply {
        count: ranked.results.len(),
        top,
        elapsed: started.elapsed(),
    })
}

pub(super) async fn check_call(base: &str) -> Finding {
    let base = trimmed(base);
    match rerank_once(base).await {
        Ok(reply) if reply.count == DOCUMENTS.len() && reply.top == Some(ANSWER) => {
            Finding::ok(format!(
                "Reranked {} passages through {base} in {} ms; the passage that answers the question came first.",
                reply.count,
                reply.elapsed.as_millis()
            ))
        }
        Ok(reply) if reply.count == DOCUMENTS.len() => Finding::failed(format!(
            "{base} reranked {} passages but put the wrong one first; check that it serves {MODEL_NAME}.",
            reply.count
        )),
        Ok(reply) => Finding::failed(format!(
            "{base} returned {} scores for {} passages.",
            reply.count,
            DOCUMENTS.len()
        )),
        Err(why) => Finding::failed(format!("Reranking through {base} failed: {why}")),
    }
}

pub(super) async fn send_settings(client: &ApiClient, url: &str) -> Vec<Finding> {
    let writes = [
        ("rerank.url", serde_json::json!(trimmed(url))),
        ("rerank.model", serde_json::json!(MODEL_NAME)),
        ("rerank.enabled", serde_json::json!(true)),
    ];
    let mut failures = Vec::new();
    for (key, value) in &writes {
        if let Err(why) = client.patch_setting(key, value.clone()).await {
            failures.push(Finding::failed(format!("The proxy refused {key}: {why}")));
        }
    }
    if failures.is_empty() {
        vec![Finding::ok(format!(
            "The running proxy accepted {} rerank settings.",
            writes.len()
        ))]
    } else {
        failures
    }
}
