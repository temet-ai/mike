use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

const PROJECT: &str = "3f0c8c62-6a3b-4a55-9d0e-2a5b7f3f6b10";

async fn repo_with(config: Option<&str>) -> anyhow::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    if let Some(config) = config {
        tokio::fs::create_dir_all(dir.path().join(".mike")).await?;
        tokio::fs::write(dir.path().join(".mike/knowledge.json"), config).await?;
    }
    Ok(dir)
}

async fn state_with_identity() -> anyhow::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    let identity = json!({
        "machine_uuid": "6f1c5c1e-3a52-4f0b-8a0d-0f8e6a4b9c11",
        "machine_name": "laptop",
    });
    tokio::fs::write(dir.path().join("machine.json"), identity.to_string()).await?;
    Ok(dir)
}

fn addr(server: &MockServer) -> String {
    server.uri().trim_start_matches("http://").to_string()
}

#[tokio::test]
async fn read_project_uuid_returns_the_uuid_from_the_configuration() -> anyhow::Result<()> {
    let repo = repo_with(Some(&format!(
        "{{\"version\": 1, \"project_uuid\": \"{PROJECT}\", \"sources\": []}}"
    )))
    .await?;
    assert_eq!(
        read_project_uuid(repo.path()).await?,
        Some(PROJECT.parse()?)
    );
    Ok(())
}

#[tokio::test]
async fn read_project_uuid_without_a_configuration_is_none() -> anyhow::Result<()> {
    let repo = repo_with(None).await?;
    assert_eq!(read_project_uuid(repo.path()).await?, None);
    Ok(())
}

#[tokio::test]
async fn read_project_uuid_rejects_an_unsupported_version() -> anyhow::Result<()> {
    let repo = repo_with(Some(&format!(
        "{{\"version\": 2, \"project_uuid\": \"{PROJECT}\"}}"
    )))
    .await?;
    let error = read_project_uuid(repo.path())
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("version 2 was accepted"))?;
    assert!(matches!(
        error,
        KnowledgeConfigError::UnsupportedVersion { found: 2, .. }
    ));
    assert!(error.to_string().contains("knowledge.json"));
    Ok(())
}

#[tokio::test]
async fn read_project_uuid_rejects_a_malformed_project_uuid() -> anyhow::Result<()> {
    let repo = repo_with(Some("{\"version\": 1, \"project_uuid\": \"project\"}")).await?;
    let error = read_project_uuid(repo.path())
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("a non-uuid project was accepted"))?;
    assert!(matches!(error, KnowledgeConfigError::Malformed { .. }));
    Ok(())
}

#[tokio::test]
async fn read_project_uuid_unreadable_configuration_is_an_io_error() -> anyhow::Result<()> {
    let repo = repo_with(None).await?;
    tokio::fs::create_dir_all(repo.path().join(".mike/knowledge.json")).await?;
    let error = read_project_uuid(repo.path())
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("a directory was read as configuration"))?;
    assert!(matches!(error, KnowledgeConfigError::Io { .. }));
    Ok(())
}

#[tokio::test]
async fn resolve_location_without_configuration_is_not_configured() -> anyhow::Result<()> {
    let repo = repo_with(None).await?;
    let outcome = resolve_location("127.0.0.1:1", repo.path(), None).await;
    assert_eq!(outcome, LocationOutcome::NotConfigured);
    Ok(())
}

#[tokio::test]
async fn resolve_location_without_a_state_directory_is_unavailable() -> anyhow::Result<()> {
    let repo = repo_with(Some(&format!(
        "{{\"version\": 1, \"project_uuid\": \"{PROJECT}\"}}"
    )))
    .await?;
    let outcome = resolve_location("127.0.0.1:1", repo.path(), None).await;
    assert!(
        matches!(outcome, LocationOutcome::Unavailable(detail) if detail.contains("XDG_STATE_HOME"))
    );
    Ok(())
}

#[tokio::test]
async fn resolve_location_returns_the_registered_location() -> anyhow::Result<()> {
    let repo = repo_with(Some(&format!(
        "{{\"version\": 1, \"project_uuid\": \"{PROJECT}\"}}"
    )))
    .await?;
    let state = state_with_identity().await?;
    let location = Uuid::new_v4();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/knowledge/locations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "project_uuid": PROJECT,
            "location_uuid": location,
            "project_path": "mike:project",
        })))
        .mount(&server)
        .await;
    let outcome = resolve_location(&addr(&server), repo.path(), Some(state.path())).await;
    assert_eq!(outcome, LocationOutcome::Bound(location));
    Ok(())
}

#[tokio::test]
async fn resolve_location_conflict_carries_the_server_message() -> anyhow::Result<()> {
    let repo = repo_with(Some(&format!(
        "{{\"version\": 1, \"project_uuid\": \"{PROJECT}\"}}"
    )))
    .await?;
    let state = state_with_identity().await?;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/knowledge/locations"))
        .respond_with(
            ResponseTemplate::new(409).set_body_string("Location belongs to another project"),
        )
        .mount(&server)
        .await;
    let outcome = resolve_location(&addr(&server), repo.path(), Some(state.path())).await;
    assert_eq!(
        outcome,
        LocationOutcome::Conflict("Location belongs to another project".into())
    );
    Ok(())
}

#[tokio::test]
async fn resolve_location_server_error_is_unavailable() -> anyhow::Result<()> {
    let repo = repo_with(Some(&format!(
        "{{\"version\": 1, \"project_uuid\": \"{PROJECT}\"}}"
    )))
    .await?;
    let state = state_with_identity().await?;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/knowledge/locations"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;
    let outcome = resolve_location(&addr(&server), repo.path(), Some(state.path())).await;
    assert!(
        matches!(outcome, LocationOutcome::Unavailable(detail) if detail.contains("500") && detail.contains("boom"))
    );
    Ok(())
}

#[tokio::test]
async fn resolve_location_unreachable_server_is_unavailable() -> anyhow::Result<()> {
    let repo = repo_with(Some(&format!(
        "{{\"version\": 1, \"project_uuid\": \"{PROJECT}\"}}"
    )))
    .await?;
    let state = state_with_identity().await?;
    let outcome = resolve_location("127.0.0.1:1", repo.path(), Some(state.path())).await;
    assert!(matches!(outcome, LocationOutcome::Unavailable(_)));
    Ok(())
}
