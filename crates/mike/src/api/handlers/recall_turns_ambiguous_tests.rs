use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::MockServer;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::seed_project;

struct Shared {
    path: String,
    owner_uuid: Uuid,
    other_uuid: Uuid,
}

async fn project_uuid(harness: &Harness, id: i64) -> anyhow::Result<Uuid> {
    Ok(
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(id)
            .fetch_one(harness.handle.pool())
            .await?,
    )
}

async fn share_a_path(harness: &Harness) -> anyhow::Result<Shared> {
    let pool = harness.handle.pool();
    let unique = Uuid::new_v4().simple().to_string();
    let path = format!("/recall-shared/{unique}/radar");
    let owner = seed_project(pool, &path).await?;
    let other = seed_project(pool, &format!("/recall-shared/{unique}/gg")).await?;
    harness
        .handle
        .add_checkout_location(Uuid::new_v4(), "host", &path, other)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the location is free"))?;
    Ok(Shared {
        path,
        owner_uuid: project_uuid(harness, owner).await?,
        other_uuid: project_uuid(harness, other).await?,
    })
}

fn assert_names_both(answer: &Value, shared: &Shared) {
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(message.contains("is ambiguous"), "{answer}");
    assert!(message.contains(&shared.owner_uuid.to_string()), "{answer}");
    assert!(message.contains(&shared.other_uuid.to_string()), "{answer}");
}

#[tokio::test]
async fn recall_similar_turns_ambiguous_project_on_the_text_path_is_a_409_with_candidates()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness
        .state
        .embedding_service()
        .apply_setting("embedding.enabled", &json!(false));
    let shared = share_a_path(&harness).await?;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": shared.path, "query": "quicksort"}),
        )
        .await?;

    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_names_both(&answer, &shared);
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_ambiguous_project_on_the_embedding_path_is_a_409_before_embedding()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let embedder = MockServer::start().await;
    let service = harness.state.embedding_service();
    service.apply_setting("embedding.url", &json!(embedder.uri()));
    service.apply_setting("embedding.enabled", &json!(true));
    let shared = share_a_path(&harness).await?;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": shared.path, "query": "quicksort"}),
        )
        .await?;

    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_names_both(&answer, &shared);
    let requests = embedder.received_requests().await.unwrap_or_default();
    assert!(requests.is_empty(), "the query was embedded: {requests:?}");
    harness.cleanup().await
}

#[tokio::test]
async fn recall_similar_turns_unknown_project_is_a_409_not_a_server_error() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;

    let (status, answer) = harness
        .post(
            "/api/recall/similar-turns",
            json!({"project": "no-such-project-anywhere", "query": "quicksort"}),
        )
        .await?;

    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert!(
        answer["error"]
            .as_str()
            .unwrap_or_default()
            .contains("Unknown project")
    );
    harness.cleanup().await
}

#[tokio::test]
async fn recall_file_history_ambiguous_project_is_a_409_with_candidates() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let shared = share_a_path(&harness).await?;

    let (status, answer) = harness
        .get(&format!(
            "/api/recall/file-history?project={}&file=src/main.rs",
            shared.path
        ))
        .await?;

    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_names_both(&answer, &shared);
    harness.cleanup().await
}

#[tokio::test]
async fn list_lessons_ambiguous_project_is_a_409_with_candidates() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let shared = share_a_path(&harness).await?;

    let (status, answer) = harness
        .get(&format!("/api/lessons?project={}", shared.path))
        .await?;

    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_names_both(&answer, &shared);
    harness.cleanup().await
}

#[tokio::test]
async fn recall_project_context_by_uuid_of_a_shared_path_project_answers() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let shared = share_a_path(&harness).await?;

    let (status, answer) = harness
        .get(&format!(
            "/api/recall/project-context?project={}",
            shared.owner_uuid
        ))
        .await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    harness.cleanup().await
}
