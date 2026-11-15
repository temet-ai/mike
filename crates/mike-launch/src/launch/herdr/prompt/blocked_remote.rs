use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const ROUTE: &str = "/api/decisions/blocked-dialog";
const TIMEOUT: Duration = Duration::from_secs(4);
const DEFAULT_URL: &str = "http://127.0.0.1:9989";
const URL_VAR: &str = "MIKE_INSPECT_URL";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockedDialogRequest {
    pub screen: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockedDialogAnswer {
    pub dialog: Option<String>,
    pub layer: String,
}

pub fn base_url() -> String {
    match std::env::var(URL_VAR) {
        Ok(url) if !url.trim().is_empty() => url.trim().trim_end_matches('/').to_owned(),
        _ => DEFAULT_URL.to_owned(),
    }
}

pub fn dialog_named_by(answer: &BlockedDialogAnswer) -> Option<String> {
    if answer.layer != "on" {
        return None;
    }
    answer
        .dialog
        .as_deref()
        .and_then(super::blocked_jev::name_of)
        .map(str::to_owned)
}

async fn fetch(base: &str, screen: &str) -> Result<BlockedDialogAnswer, String> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .no_proxy()
        .build()
        .map_err(|e| format!("could not build an HTTP client: {e}"))?;
    let body = BlockedDialogRequest {
        screen: super::blocked_jev::tail(screen),
        session: None,
    };
    let response = client
        .post(format!("{base}{ROUTE}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("the proxy at {base} did not answer: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the proxy at {base} answered {status}"));
    }
    response
        .json()
        .await
        .map_err(|e| format!("the proxy at {base} sent an unreadable answer: {e}"))
}

pub async fn ask(base: &str, screen: &str) -> Option<String> {
    match fetch(base, screen).await {
        Ok(answer) => dialog_named_by(&answer),
        Err(why) => {
            tracing::debug!(%why, "blocked dialog: serve did not name the dialog");
            None
        }
    }
}

#[cfg(test)]
#[path = "blocked_remote_tests.rs"]
mod tests;
