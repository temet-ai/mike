use crate::db_helpers;

use chrono::Utc;
use mike::store::persistence::{PersistHandle, SessionEmbeddingWrite};

use super::support::{planar_unit, seed_project, seed_session, set_narrative_overview};

#[tokio::test]
async fn session_overview_carries_embedding_knowledge_and_related_sessions() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&proxy.test_db.database_url).await?;

    let project_id = seed_project(&proxy.pool, "overview-extension-test").await?;
    let (a_pk, a_uuid) = seed_session(&proxy.pool, project_id, Some("Session A")).await?;
    let (_b_pk, b_uuid) = seed_session(&proxy.pool, project_id, Some("Session B")).await?;
    set_narrative_overview(
        &proxy.pool,
        a_pk,
        "The session fixed the bug.\n\nThe fix shipped and tests pass.",
    )
    .await?;

    let now = Utc::now();
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: a_uuid,
            embedding: &planar_unit(0.0),
            model: "test-model",
            source: "overview",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: b_uuid,
            embedding: &planar_unit(4.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;
    persist
        .upsert_session_knowledge(
            a_uuid,
            &["bug fix".to_string(), "session judge".to_string()],
            &["the fix shipped cleanly".to_string()],
            &[b_uuid],
            None,
        )
        .await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/sessions/{a_uuid}/overview",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;

    assert_eq!(body["embedding"]["present"], true);
    assert_eq!(body["embedding"]["source"], "overview");

    assert_eq!(body["knowledge"]["topics"][0], "bug fix");
    assert_eq!(body["knowledge"]["learnings"][0], "the fix shipped cleanly");

    let related = body["related_sessions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("related_sessions must be an array: {body}"))?;
    assert!(
        related
            .iter()
            .any(|r| r["session_id"] == b_uuid.to_string()),
        "neighbour must appear in related_sessions: {body}"
    );

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn session_overview_knowledge_key_is_present_and_null_when_unjudged() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };

    let project_id = seed_project(&proxy.pool, "overview-no-knowledge-test").await?;
    let (_pk, session_uuid) =
        seed_session(&proxy.pool, project_id, Some("Unjudged session")).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/sessions/{session_uuid}/overview",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;

    let knowledge = body
        .get("knowledge")
        .ok_or_else(|| anyhow::anyhow!("knowledge key must be present, not omitted: {body}"))?;
    assert!(
        knowledge.is_null(),
        "knowledge must serialize as JSON null when unjudged, got {knowledge}: {body}"
    );

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn create_lesson_with_session_id_returns_related_sessions_and_neighbour_lessons()
-> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&proxy.test_db.database_url).await?;

    let project_id = seed_project(&proxy.pool, "lesson-extension-test").await?;
    let (a_pk, a_uuid) = seed_session(&proxy.pool, project_id, Some("Session A")).await?;
    let (_b_pk, b_uuid) = seed_session(&proxy.pool, project_id, Some("Session B")).await?;

    let now = Utc::now();
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: a_uuid,
            embedding: &planar_unit(0.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;
    persist
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: b_uuid,
            embedding: &planar_unit(4.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 1,
            source_updated_at: now,
        })
        .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/lessons", proxy.inspect_url))
        .json(&serde_json::json!({
            "project_path": "lesson-extension-test",
            "kind": "correction",
            "text": "always ack mailbox delivery before clearing the sender",
            "session_id": b_uuid.to_string(),
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);

    let resp = client
        .post(format!("{}/api/lessons", proxy.inspect_url))
        .json(&serde_json::json!({
            "project_path": "lesson-extension-test",
            "kind": "correction",
            "text": "a second, unrelated lesson",
            "session_id": a_uuid.to_string(),
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);
    let body: serde_json::Value = resp.json().await?;

    let related = body["related_sessions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("related_sessions must be an array: {body}"))?;
    assert!(
        related
            .iter()
            .any(|r| r["session_id"] == b_uuid.to_string()),
        "session B must appear as a related session: {body}"
    );

    let neighbour_lessons = body["neighbour_lessons"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("neighbour_lessons must be an array: {body}"))?;
    assert!(
        neighbour_lessons
            .iter()
            .any(|l| l["text"] == "always ack mailbox delivery before clearing the sender"),
        "session B's lesson must appear as a neighbour lesson: {body}"
    );

    let _ = a_pk;
    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn create_lesson_without_session_id_returns_empty_related_arrays() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/lessons", proxy.inspect_url))
        .json(&serde_json::json!({
            "project_path": "lesson-no-session-test",
            "kind": "convention",
            "text": "a lesson with no session_id",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["related_sessions"].as_array().map(Vec::len), Some(0));
    assert_eq!(body["neighbour_lessons"].as_array().map(Vec::len), Some(0));

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn list_projects_carries_project_id() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let project_id = seed_project(&proxy.pool, "project-id-field-test").await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/projects", proxy.inspect_url))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    let entries = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("response must be an array: {body}"))?;
    let entry = entries
        .iter()
        .find(|e| e["project_path"] == "project-id-field-test")
        .ok_or_else(|| anyhow::anyhow!("seeded project must appear: {body}"))?;
    assert_eq!(entry["project_id"], project_id);

    proxy.shutdown.cancel();
    Ok(())
}
