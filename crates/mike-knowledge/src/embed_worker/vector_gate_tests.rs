use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::*;
use crate::analysis::EmbeddingConfig;
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::persistence::test_support::{
    acquire_ephemeral_db, seed_project, seed_session_turns,
};

struct Fed {
    feed: ChangeFeed,
    shutdown: CancellationToken,
    _events: broadcast::Sender<crate::store::models::ProxyEvent>,
}

impl Drop for Fed {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

fn fed(persist: &PersistHandle) -> Fed {
    let feed = ChangeFeed::new();
    let (events, rx) = broadcast::channel(16);
    let shutdown = CancellationToken::new();
    spawn_change_feed(
        &feed,
        Some(persist),
        rx,
        shutdown.clone(),
        &TaskTracker::new(),
    );
    Fed {
        feed,
        shutdown,
        _events: events,
    }
}

fn embedding() -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: "http://127.0.0.1:9".to_owned(),
        model: format!("vector-gate-{}", uuid::Uuid::new_v4()),
        chunk_max_chars: 80,
    })
}

async fn session(pool: &PgPool) -> anyhow::Result<(i64, String)> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let project = seed_project(pool, &format!("/vector-gate/{uuid}")).await?;
    let id = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(&uuid)
    .bind(project)
    .fetch_one(pool)
    .await?;
    seed_session_turns(pool, &uuid, project, 1).await?;
    Ok((id, uuid))
}

async fn embedded_chunk(pool: &PgPool, session: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
                (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model,
                 embedded_at)
         SELECT t.id, 0, (SELECT id FROM memory.chunk_roles WHERE code = 'prompt'), 'chunk',
                array_fill(0.1::real, ARRAY[768])::vector, 'test-model', NOW()
           FROM memory.operator_turns t
          WHERE t.session_id = $1",
    )
    .bind(session)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn found_nothing_waits_until_a_session_embedding_source_changes() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let fed = fed(&persist);
    let embedding = embedding();
    let pool = PgPool::connect(&db.database_url).await?;
    refresh_session_vectors_when_owed(&persist, &embedding).await?;
    assert!(persist.is_fresh(NO_SESSION_VECTOR_OWED, SESSION_VECTOR_SOURCES, Utc::now()));
    let (session, _) = session(&pool).await?;
    embedded_chunk(&pool, session).await?;
    fed.feed
        .record(crate::store::models::RowChangedTable::Requests, None);
    let waited = refresh_session_vectors_when_owed(&persist, &embedding).await?;
    assert_eq!(
        waited.written(),
        0,
        "a request change does not wake the sweep"
    );
    fed.feed
        .record(RowChangedTable::SessionEmbeddingSources, None);
    let woke = refresh_session_vectors_when_owed(&persist, &embedding).await?;
    assert_eq!(woke.centroid_written, 1);
    assert_eq!(woke.projects_refreshed, 1);
    let settled = refresh_session_vectors_when_owed(&persist, &embedding).await?;
    assert_eq!((settled.written(), settled.projects_refreshed), (0, 0));
    pool.close().await;
    db.cleanup().await
}

#[tokio::test]
async fn project_centroids_wait_until_a_session_embedding_changes() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let fed = fed(&persist);
    let now = Utc::now();
    assert_eq!(stale_project_centroids(&persist, false, now).await, 0);
    assert!(persist.is_fresh(PROJECT_CENTROIDS_CURRENT, SESSION_VECTORS, now));
    fed.feed
        .record(RowChangedTable::SessionEmbeddingSources, None);
    assert!(persist.is_fresh(PROJECT_CENTROIDS_CURRENT, SESSION_VECTORS, now));
    fed.feed.record(RowChangedTable::SessionEmbeddings, None);
    assert!(!persist.is_fresh(PROJECT_CENTROIDS_CURRENT, SESSION_VECTORS, now));
    let later = now + VECTOR_BACKSTOP;
    stale_project_centroids(&persist, false, now).await;
    assert!(!persist.is_fresh(PROJECT_CENTROIDS_CURRENT, SESSION_VECTORS, later));
    db.cleanup().await
}

#[tokio::test]
async fn sweeps_every_pass_without_a_change_feed() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let embedding = embedding();
    let pool = PgPool::connect(&db.database_url).await?;
    refresh_session_vectors_when_owed(&persist, &embedding).await?;
    let (session, _) = session(&pool).await?;
    embedded_chunk(&pool, session).await?;
    let swept = refresh_session_vectors_when_owed(&persist, &embedding).await?;
    assert_eq!(swept.centroid_written, 1);
    pool.close().await;
    db.cleanup().await
}

#[derive(Debug, Deserialize)]
struct RowChanged {
    table: RowChangedTable,
    id: String,
    session_id: Option<String>,
}

#[tokio::test]
async fn chunk_marks_and_overviews_notify_and_a_status_change_stays_silent() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let pool = PgPool::connect(&db.database_url).await?;
    let (session, uuid) = session(&pool).await?;
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    embedded_chunk(&pool, session).await?;
    let changed: RowChanged = serde_json::from_str(listener.recv().await?.payload())?;
    assert_eq!(changed.table, RowChangedTable::SessionEmbeddingSources);
    assert_eq!(changed.id, session.to_string());
    assert_eq!(changed.session_id.as_deref(), Some(uuid.as_str()));
    sqlx::query(
        "UPDATE sessions.sessions SET overview = 'o', overview_generated_at = NOW() WHERE id = $1",
    )
    .bind(session)
    .execute(&pool)
    .await?;
    let overview: RowChanged = serde_json::from_str(listener.recv().await?.payload())?;
    let sessions: RowChanged = serde_json::from_str(listener.recv().await?.payload())?;
    let mut tables = [overview.table, sessions.table];
    tables.sort_by_key(|t| format!("{t:?}"));
    assert_eq!(
        tables,
        [
            RowChangedTable::SessionEmbeddingSources,
            RowChangedTable::Sessions
        ]
    );
    sqlx::query("UPDATE sessions.sessions SET last_status = 'idle' WHERE id = $1")
        .bind(session)
        .execute(&pool)
        .await?;
    let status: RowChanged = serde_json::from_str(listener.recv().await?.payload())?;
    assert_eq!(status.table, RowChangedTable::Sessions);
    sqlx::query("SELECT pg_notify('proxy.row_changed', 'marker')")
        .execute(&pool)
        .await?;
    assert_eq!(listener.recv().await?.payload(), "marker");
    pool.close().await;
    db.cleanup().await
}
