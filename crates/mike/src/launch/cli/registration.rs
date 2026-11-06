use std::path::Path;

use uuid::Uuid;

use super::knowledge_location::{LocationOutcome, resolve_location};
use crate::machine_identity;

pub(super) async fn register_opened_surface(
    inspect_addr: Option<&str>,
    session: Uuid,
    folder: &Path,
    repo_root: Option<&Path>,
    pid: Option<u32>,
) {
    let state_dir = machine_identity::default_state_dir().ok();
    register_with_state_dir(
        inspect_addr,
        session,
        folder,
        repo_root,
        pid,
        state_dir.as_deref(),
    )
    .await;
}

async fn register_with_state_dir(
    inspect_addr: Option<&str>,
    session: Uuid,
    folder: &Path,
    repo_root: Option<&Path>,
    pid: Option<u32>,
    state_dir: Option<&Path>,
) {
    let Some(inspect_addr) = inspect_addr else {
        return;
    };
    let location = match repo_root {
        Some(root) => report_location(
            session,
            root,
            resolve_location(inspect_addr, root, state_dir).await,
        ),
        None => None,
    };
    let mut body = serde_json::json!({
        "session_id": session,
        "cwd": folder.to_string_lossy(),
        "repo_root": repo_root.map(|p| p.to_string_lossy().into_owned()),
        "registered_via": "launcher",
        "pid": pid,
    });
    if let Some(location) = location {
        body["location_uuid"] = serde_json::json!(location);
    }
    post_registration(inspect_addr, session, &body).await;
}

fn report_location(session: Uuid, root: &Path, outcome: LocationOutcome) -> Option<Uuid> {
    match outcome {
        LocationOutcome::NotConfigured => None,
        LocationOutcome::Bound(location) => Some(location),
        LocationOutcome::Conflict(detail) => {
            eprintln!(
                "mike spawn: {} is already registered on this machine as a location of a different \
                 project ({detail}); {session} is registered without a knowledge location and \
                 keeps its path-derived project",
                root.display()
            );
            None
        }
        LocationOutcome::Unavailable(detail) => {
            eprintln!(
                "mike spawn: could not attach {session} to its knowledge location ({detail}); the \
                 proxy binds it from the first captured request when this checkout is a \
                 registered location on the proxy host"
            );
            None
        }
    }
}

async fn post_registration(inspect_addr: &str, session: Uuid, body: &serde_json::Value) {
    let url = format!("http://{inspect_addr}/api/workspace/register");
    match reqwest::Client::new().post(&url).json(body).send().await {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            eprintln!(
                "mike spawn: workspace registration for {session} answered {status} {detail}; \
                 the roster will pick the session up from its first captured request instead"
            );
        }
        Err(e) => eprintln!(
            "mike spawn: could not register {session} on the workspace roster ({e}); the \
             roster will pick the session up from its first captured request instead"
        ),
    }
}

#[cfg(test)]
mod tests;
