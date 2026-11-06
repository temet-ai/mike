use crate::db_helpers;
use crate::seed;

use chrono::Utc;
use mike::store::persistence::{PersistHandle, SessionEmbeddingWrite};
use uuid::Uuid;

use super::support::{planar_unit, seed_project, seed_session};

#[tokio::test]
async fn session_embedding_graph_populated_returns_centre_and_neighbour() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&proxy.test_db.database_url).await?;

    let project_id = seed_project(&proxy.pool, "graph-api-populated").await?;
    let (_a_pk, a_uuid) = seed_session(&proxy.pool, project_id, Some("Session A")).await?;
    let (_b_pk, b_uuid) = seed_session(&proxy.pool, project_id, Some("Session B")).await?;

    sqlx::query("UPDATE sessions.sessions SET last_status = 'Complete' WHERE session_uuid = $1")
        .bind(b_uuid)
        .execute(&proxy.pool)
        .await?;

    let now = Utc::now();
    persist
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
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: b_uuid,
            embedding: &planar_unit(5.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 2,
            source_updated_at: now,
        })
        .await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/sessions/{a_uuid}/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["session_id"], a_uuid.to_string());
    assert_eq!(body["embedding"]["present"], true);
    assert_eq!(body["embedding"]["source"], "centroid");

    let nodes = body["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("nodes must be an array: {body}"))?;
    let centre = nodes
        .iter()
        .find(|n| n["is_centre"] == true && n["session_id"] == a_uuid.to_string())
        .ok_or_else(|| anyhow::anyhow!("centre node must be present: {body}"))?;
    assert_eq!(
        centre["last_status"], "",
        "a session with no cached status must serialize last_status as \"\", never null: {body}"
    );
    let neighbour = nodes
        .iter()
        .find(|n| n["session_id"] == b_uuid.to_string())
        .ok_or_else(|| anyhow::anyhow!("neighbour node must be present: {body}"))?;
    assert_eq!(
        neighbour["last_status"], "Complete",
        "last_status must carry the neighbour's real status onto the wire: {body}"
    );

    let edges = body["edges"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("edges must be an array: {body}"))?;
    assert!(
        edges
            .iter()
            .any(|e| e["from"] == a_uuid.to_string() && e["to"] == b_uuid.to_string()),
        "centre->neighbour edge must be present: {body}"
    );

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn session_embedding_graph_last_status_prefers_live_request_over_stale_cache()
-> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&proxy.test_db.database_url).await?;

    let project_id = seed_project(&proxy.pool, "graph-api-live-status").await?;
    let (_a_pk, a_uuid) = seed_session(&proxy.pool, project_id, Some("Session A")).await?;
    let (b_pk, b_uuid) = seed_session(&proxy.pool, project_id, Some("Session B")).await?;

    sqlx::query("UPDATE sessions.sessions SET last_status = 'Complete' WHERE session_uuid = $1")
        .bind(b_uuid)
        .execute(&proxy.pool)
        .await?;

    let now = Utc::now();
    seed::SeedRequest::new(9_991_001, project_id)
        .session_pk(b_pk)
        .status("streaming")
        .created_at(now)
        .insert(&proxy.pool)
        .await?;

    persist
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
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id: b_uuid,
            embedding: &planar_unit(5.0),
            model: "test-model",
            source: "centroid",
            chunk_count: 2,
            source_updated_at: now,
        })
        .await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/sessions/{a_uuid}/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    let nodes = body["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("nodes must be an array: {body}"))?;
    let neighbour = nodes
        .iter()
        .find(|n| n["session_id"] == b_uuid.to_string())
        .ok_or_else(|| anyhow::anyhow!("neighbour node must be present: {body}"))?;
    assert_eq!(
        neighbour["last_status"], "Streaming",
        "the live newest_status LATERAL must win over the stale sessions.sessions cache \
         column — a wire value of \"Complete\" here means the LATERAL isn't running: {body}"
    );

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn session_embedding_graph_no_vector_returns_centre_only() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let project_id = seed_project(&proxy.pool, "graph-api-no-vector").await?;
    let (_pk, session_uuid) = seed_session(&proxy.pool, project_id, Some("No Vector")).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/sessions/{session_uuid}/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["embedding"]["present"], false);
    assert!(body["embedding"]["reason"].is_string());
    let nodes = body["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("nodes must be an array: {body}"))?;
    assert_eq!(nodes.len(), 1, "only the centre node, got: {body}");
    assert_eq!(nodes[0]["is_centre"], true);
    let edges = body["edges"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("edges must be an array: {body}"))?;
    assert!(edges.is_empty());

    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn session_embedding_graph_unknown_session_is_404() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let unknown = Uuid::new_v4();
    let resp = client
        .get(format!(
            "{}/api/sessions/{unknown}/embedding-graph",
            proxy.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    proxy.shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn session_embedding_graph_bad_params_are_400() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_db("http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let project_id = seed_project(&proxy.pool, "graph-api-bad-params").await?;
    let (_pk, session_uuid) = seed_session(&proxy.pool, project_id, Some("Bad Params")).await?;

    let client = reqwest::Client::new();
    let base = format!(
        "{}/api/sessions/{session_uuid}/embedding-graph",
        proxy.inspect_url
    );

    let resp = client.get(format!("{base}?k=0")).send().await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "k=0 must be rejected"
    );

    let resp = client.get(format!("{base}?k=33")).send().await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "k=33 must be rejected"
    );

    let resp = client
        .get(format!("{base}?scope=everywhere"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "an unknown scope must be rejected"
    );

    let resp = client
        .get(format!("{base}?min_similarity=1.5"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "min_similarity out of [0,1] must be rejected"
    );

    proxy.shutdown.cancel();
    Ok(())
}
