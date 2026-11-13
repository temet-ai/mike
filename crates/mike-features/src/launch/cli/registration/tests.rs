use std::path::PathBuf;

use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

const PROJECT: &str = "3f0c8c62-6a3b-4a55-9d0e-2a5b7f3f6b10";
const MACHINE: &str = "6f1c5c1e-3a52-4f0b-8a0d-0f8e6a4b9c11";

fn addr(server: &MockServer) -> String {
    server.uri().trim_start_matches("http://").to_string()
}

async fn checkout(with_config: bool) -> anyhow::Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().join("repo");
    tokio::fs::create_dir_all(repo.join(".mike")).await?;
    if with_config {
        let config = json!({"version": 1, "project_uuid": PROJECT});
        tokio::fs::write(repo.join(".mike/knowledge.json"), config.to_string()).await?;
    }
    Ok((dir, repo))
}

async fn state_dir_with_identity(dir: &tempfile::TempDir) -> anyhow::Result<PathBuf> {
    let state = dir.path().join("state");
    tokio::fs::create_dir_all(&state).await?;
    let identity = json!({"machine_uuid": MACHINE, "machine_name": "laptop"});
    tokio::fs::write(state.join("machine.json"), identity.to_string()).await?;
    Ok(state)
}

#[tokio::test]
async fn register_opened_surface_no_inspect_addr_is_a_silent_no_op() {
    register_opened_surface(None, Uuid::new_v4(), Path::new("/tmp/x"), None, Some(123)).await;
}

#[tokio::test]
async fn register_opened_surface_posts_expected_body() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let session = Uuid::new_v4();
    let expected = json!({
        "session_id": session,
        "cwd": "/repo/proj",
        "repo_root": "/repo",
        "registered_via": "launcher",
        "pid": 4242,
    });
    Mock::given(method("POST"))
        .and(path("/api/workspace/register"))
        .and(body_json(&expected))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    register_opened_surface(
        Some(&addr(&server)),
        session,
        Path::new("/repo/proj"),
        Some(Path::new("/repo")),
        Some(4242),
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn register_with_state_dir_passes_the_resolved_location_to_registration() -> anyhow::Result<()>
{
    let (dir, repo) = checkout(true).await?;
    let state = state_dir_with_identity(&dir).await?;
    let root = tokio::fs::canonicalize(&repo).await?;
    let location = Uuid::new_v4();
    let session = Uuid::new_v4();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/knowledge/locations"))
        .and(body_json(json!({
            "project": PROJECT,
            "machine_uuid": MACHINE,
            "machine_name": "laptop",
            "root_path": root.to_string_lossy(),
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "project_uuid": PROJECT,
            "location_uuid": location,
            "project_path": format!("mike:{PROJECT}"),
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/workspace/register"))
        .and(body_partial_json(json!({
            "session_id": session,
            "location_uuid": location,
            "registered_via": "launcher",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    register_with_state_dir(
        Some(&addr(&server)),
        session,
        &repo,
        Some(&repo),
        None,
        Some(&state),
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn register_with_state_dir_location_conflict_registers_without_a_location()
-> anyhow::Result<()> {
    let (dir, repo) = checkout(true).await?;
    let state = state_dir_with_identity(&dir).await?;
    let session = Uuid::new_v4();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/knowledge/locations"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": "Location belongs to another project"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/workspace/register"))
        .and(body_json(json!({
            "session_id": session,
            "cwd": repo.to_string_lossy(),
            "repo_root": repo.to_string_lossy(),
            "registered_via": "launcher",
            "pid": null,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    register_with_state_dir(
        Some(&addr(&server)),
        session,
        &repo,
        Some(&repo),
        None,
        Some(&state),
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn register_with_state_dir_without_machine_identity_skips_the_location_call()
-> anyhow::Result<()> {
    let (dir, repo) = checkout(true).await?;
    let empty_state = dir.path().join("no-identity");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/knowledge/locations"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/workspace/register"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    register_with_state_dir(
        Some(&addr(&server)),
        Uuid::new_v4(),
        &repo,
        Some(&repo),
        None,
        Some(&empty_state),
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn register_with_state_dir_without_knowledge_config_skips_the_location_call()
-> anyhow::Result<()> {
    let (dir, repo) = checkout(false).await?;
    let state = state_dir_with_identity(&dir).await?;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/knowledge/locations"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/workspace/register"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    register_with_state_dir(
        Some(&addr(&server)),
        Uuid::new_v4(),
        &repo,
        Some(&repo),
        None,
        Some(&state),
    )
    .await;
    Ok(())
}
