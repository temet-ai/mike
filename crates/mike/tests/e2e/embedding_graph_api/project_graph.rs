use crate::db_helpers;
use crate::seed;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;

use chrono::Utc;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::session_embeddings::SessionEmbeddingWrite;

use super::support::{planar_unit, seed_project, seed_session};

#[tokio::test]
async fn project_embedding_graph_populated_returns_nodes_and_siblings() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&proxy.test_db.database_url).await?;

    let project_id = seed_project(&proxy.pool, "project-graph-populated").await?;
    let sibling_project_id = seed_project(&proxy.pool, "project-graph-sibling").await?;
    let (_a_pk, a_uuid) = seed_session(&proxy.pool, project_id, Some("Session A")).await?;
    let (_b_pk, b_uuid) = seed_session(&proxy.pool, project_id, Some("Session B")).await?;
    let (_c_pk, c_uuid) = seed_session(&proxy.pool, sibling_project_id, Some("Session C")).await?;

    sqlx::query("UPDATE sessions.sessions SET last_status = 'Streaming' WHERE session_uuid = $1")
        .bind(a_uuid)
        .execute(&proxy.pool)
        .await?;

    let now = Utc::now();
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: a_uuid,
            embedding: &planar_unit(0.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 2,
            source_updated_at: now,
        })
        .await?;
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: b_uuid,
            embedding: &planar_unit(3.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 2,
            source_updated_at: now,
        })
        .await?;
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: c_uuid,
            embedding: &planar_unit(2.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 2,
            source_updated_at: now,
        })
        .await?;
    persist.knowledge().refresh_project_centroids().await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/projects/{project_id}/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["project_id"], project_id);
    assert_eq!(body["project_path"], "project-graph-populated");
    assert_eq!(body["centroid"]["present"], true);

    let nodes = body["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("nodes must be an array: {body}"))?;
    assert_eq!(
        nodes.len(),
        2,
        "both project-A sessions must be nodes: {body}"
    );
    assert!(nodes.iter().all(|n| n["session_id"] != c_uuid.to_string()));
    let a_node = nodes
        .iter()
        .find(|n| n["session_id"] == a_uuid.to_string())
        .ok_or_else(|| anyhow::anyhow!("session A must be a node: {body}"))?;
    assert_eq!(
        a_node["last_status"], "Streaming",
        "last_status must carry the session's real status onto the wire: {body}"
    );

    let siblings = body["sibling_projects"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("sibling_projects must be an array: {body}"))?;
    assert!(
        siblings
            .iter()
            .any(|s| s["project_id"] == sibling_project_id),
        "the sibling project must appear: {body}"
    );

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn project_embedding_graph_last_status_prefers_live_request_over_stale_cache()
-> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&proxy.test_db.database_url).await?;

    let project_id = seed_project(&proxy.pool, "project-graph-live-status").await?;
    let (a_pk, a_uuid) = seed_session(&proxy.pool, project_id, Some("Session A")).await?;
    let (_b_pk, b_uuid) = seed_session(&proxy.pool, project_id, Some("Session B")).await?;

    sqlx::query(
        "UPDATE sessions.sessions SET last_status = 'Error: stale' WHERE session_uuid = $1",
    )
    .bind(a_uuid)
    .execute(&proxy.pool)
    .await?;

    let now = Utc::now();
    seed::SeedRequest::new(9_992_001, project_id)
        .session_pk(a_pk)
        .status("pending")
        .created_at(now)
        .insert(&proxy.pool)
        .await?;

    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: a_uuid,
            embedding: &planar_unit(0.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 2,
            source_updated_at: now,
        })
        .await?;
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: b_uuid,
            embedding: &planar_unit(3.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 2,
            source_updated_at: now,
        })
        .await?;
    persist.knowledge().refresh_project_centroids().await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/projects/{project_id}/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    let nodes = body["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("nodes must be an array: {body}"))?;
    let a_node = nodes
        .iter()
        .find(|n| n["session_id"] == a_uuid.to_string())
        .ok_or_else(|| anyhow::anyhow!("session A must be a node: {body}"))?;
    assert_eq!(
        a_node["last_status"], "Pending",
        "the live newest_status LATERAL must win over the stale sessions.sessions cache \
         column — a wire value of \"Error: stale\" here means the LATERAL isn't running: {body}"
    );

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn project_embedding_graph_no_vectors_returns_empty_arrays() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let project_id = seed_project(&proxy.pool, "project-graph-no-vectors").await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/projects/{project_id}/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["centroid"]["present"], false);
    assert_eq!(body["nodes"].as_array().map(Vec::len), Some(0));
    assert_eq!(body["edges"].as_array().map(Vec::len), Some(0));
    assert_eq!(body["sibling_projects"].as_array().map(Vec::len), Some(0));

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn project_embedding_graph_unknown_project_is_404() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/projects/999999999/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn project_embedding_graph_bad_params_are_400() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let project_id = seed_project(&proxy.pool, "project-graph-bad-params").await?;

    let client = reqwest::Client::new();
    let base = format!(
        "{}/api/projects/{project_id}/embedding-graph",
        proxy.inspect_url
    );

    let resp = client.get(format!("{base}?limit=0")).send().await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "limit=0 must be rejected"
    );

    let resp = client
        .get(format!("{base}?min_similarity=-0.5"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "negative min_similarity must be rejected"
    );

    let resp = client
        .get(format!(
            "{}/api/projects/not-a-number/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "non-numeric id must be rejected"
    );

    proxy.shutdown.cancel();
    Ok(())
}
