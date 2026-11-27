use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::cli::{HerdrCli, str_field};
use crate::launch::LaunchError;

fn native_session(agent: &Value) -> Option<&str> {
    let session = agent.get("agent_session")?;
    if str_field(session, "kind").is_some_and(|kind| kind != "id") {
        return None;
    }
    str_field(session, "value")
}

pub(super) fn claude_session(agent: &Value) -> Option<Uuid> {
    (str_field(agent, "agent") == Some("claude"))
        .then(|| native_session(agent)?.parse().ok())
        .flatten()
}

pub(super) async fn session(cli: &HerdrCli, agent: &Value) -> Result<Option<Uuid>, LaunchError> {
    if str_field(agent, "agent") != Some("codex") {
        return Ok(claude_session(agent));
    }
    let Some(thread) = native_session(agent) else {
        return Ok(None);
    };
    let client = mike_base::http_client::builder("herdr-codex-identity")
        .timeout(Duration::from_secs(4))
        .no_proxy()
        .build()
        .map_err(failed)?;
    let response = client
        .get(format!(
            "{}/api/codex/session",
            cli.inspect_url.trim_end_matches('/')
        ))
        .query(&[("thread_id", thread)])
        .send()
        .await
        .map_err(failed)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    #[derive(Deserialize)]
    struct Binding {
        session_id: Uuid,
    }
    let binding: Binding = response
        .error_for_status()
        .map_err(failed)?
        .json()
        .await
        .map_err(failed)?;
    Ok(Some(binding.session_id))
}

fn failed(error: reqwest::Error) -> LaunchError {
    LaunchError::Failed(format!(
        "resolving the Herdr Codex session through Mike: {error}"
    ))
}
