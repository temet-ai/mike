use crate::test_db;

use super::fixtures::{make_pool_and_persist, planar_unit, seed_project, seed_session};
use anyhow::Context;
use chrono::{DateTime, Duration as ChronoDuration, SubsecRound, Utc};
use mike::store::persistence::SessionEmbeddingWrite;
use test_db::TestDb;
use uuid::Uuid;

struct GraphFixtureUuids {
    a: Uuid,
    b: Uuid,
    c: Uuid,
    d: Uuid,
}

async fn seed_graph_fixture(
    pool: &sqlx::PgPool,
    persist: &mike::store::persistence::PersistHandle,
    project_id: i64,
    now: DateTime<Utc>,
) -> anyhow::Result<GraphFixtureUuids> {
    let (_a_pk, a_uuid) = seed_session(pool, project_id, Some("A"), now).await?;
    let (_b_pk, b_uuid) = seed_session(
        pool,
        project_id,
        Some("B"),
        now - ChronoDuration::seconds(1),
    )
    .await?;
    let (_c_pk, c_uuid) = seed_session(
        pool,
        project_id,
        Some("C"),
        now - ChronoDuration::seconds(2),
    )
    .await?;
    let (_d_pk, d_uuid) = seed_session(
        pool,
        project_id,
        Some("D"),
        now - ChronoDuration::seconds(3),
    )
    .await?;

    sqlx::query("UPDATE sessions.sessions SET last_status = 'Streaming' WHERE session_uuid = $1")
        .bind(a_uuid)
        .execute(pool)
        .await?;

    for (uuid, angle) in [
        (a_uuid, 0.0),
        (b_uuid, 10.0),
        (c_uuid, 170.0),
        (d_uuid, 180.0),
    ] {
        persist
            .upsert_session_embedding(SessionEmbeddingWrite {
                session_id: uuid,
                embedding: &planar_unit(angle),
                model: "test-model",
                source: "centroid",
                chunk_count: 1,
                source_updated_at: now,
            })
            .await?;
    }

    Ok(GraphFixtureUuids {
        a: a_uuid,
        b: b_uuid,
        c: c_uuid,
        d: d_uuid,
    })
}

fn normalize_pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b { (a, b) } else { (b, a) }
}

#[tokio::test]
async fn project_session_graph_returns_undirected_deduplicated_edges() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping project_session_graph_returns_undirected_deduplicated_edges"
        );
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id = seed_project(&pool, &format!("graph-test-{}", uuid::Uuid::new_v4())).await?;
    let now = Utc::now().trunc_subsecs(6);

    let uuids = seed_graph_fixture(&pool, &persist, project_id, now).await?;

    let (nodes, edges) = persist
        .project_session_graph(project_id, 10, 0.0, 1)
        .await?;

    assert_eq!(
        nodes.len(),
        4,
        "all four sessions have vectors and fit under limit=10"
    );
    let a_node = nodes
        .iter()
        .find(|n| n.session_id == uuids.a)
        .context("A present in nodes")?;
    assert_eq!(
        a_node.last_status.as_deref(),
        Some("Streaming"),
        "last_status must surface the sessions.sessions cache column"
    );
    let b_node = nodes
        .iter()
        .find(|n| n.session_id == uuids.b)
        .context("B present in nodes")?;
    assert_eq!(
        b_node.last_status, None,
        "a session with no cached status and no request rows must come back None, not a bogus default"
    );

    let mut edge_pairs: Vec<(Uuid, Uuid)> = edges
        .iter()
        .map(|e| normalize_pair(e.from_session_id, e.to_session_id))
        .collect();
    edge_pairs.sort();

    let mut expected = vec![
        normalize_pair(uuids.a, uuids.b),
        normalize_pair(uuids.c, uuids.d),
    ];
    expected.sort();

    assert_eq!(
        edge_pairs, expected,
        "edges must be exactly the two mutual-nearest pairs, undirected and deduplicated"
    );
    assert_eq!(edges.len(), 2, "no duplicate reverse-direction edge");

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
