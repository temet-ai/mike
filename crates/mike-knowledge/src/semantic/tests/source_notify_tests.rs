use serde::Deserialize;
use sqlx::PgPool;
use sqlx::postgres::PgListener;

use crate::store::models::RowChangedTable;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};

#[derive(Debug, Deserialize)]
struct RowChanged {
    table: RowChangedTable,
    id: String,
    session_id: Option<String>,
}

async fn session(pool: &PgPool) -> anyhow::Result<(i64, String)> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let project = seed_project(pool, &format!("/source-notify/{uuid}")).await?;
    let id = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(&uuid)
    .bind(project)
    .fetch_one(pool)
    .await?;
    Ok((id, uuid))
}

async fn entry(pool: &PgPool, session_id: i64, kind: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.dossier_entries
                (session_id, kind, data, source, source_ref, dedup_key, extractor_version, effective_at)
         VALUES ($1, $2, '{}'::jsonb, 'capture', 'requests.requests:1', $2 || ':' || $1, 'test', NOW())
         RETURNING id",
    )
    .bind(session_id)
    .bind(kind)
    .fetch_one(pool)
    .await?)
}

async fn embedder(pool: &PgPool) -> anyhow::Result<String> {
    let key = format!("source-notify|{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO memory.semantic_embedders
                (embedder_key, model, dim, chunk_max_chars, prefix_version, urls_seen,
                 canary_version, canary_vectors, status)
         VALUES ($1, 'test-model', 768, 1024, 'doc-prefix-v1', '{}', 'v1', '{}'::jsonb, 'retired')",
    )
    .bind(&key)
    .execute(pool)
    .await?;
    Ok(key)
}

async fn item(pool: &PgPool, key: &str, kind: &str, source_ref: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.semantic_items
                (kind, origin_session_uuid, origin_turn_id, source_ref, pattern, text, text_chars,
                 embedding, embedder_key, vector_source, builder_version, effective_at, data)
         SELECT $1, CASE WHEN a THEN gen_random_uuid() END, CASE WHEN a THEN 1 END, $2,
                CASE WHEN a THEN 'p' END, 'text', 4, array_fill(0.1::real, ARRAY[768])::vector,
                $3, 'embedded', 'test', NOW(), '{\"text_source\": \"operator_quote\"}'::jsonb
           FROM (SELECT $1 = 'anchor' AS a) k
         RETURNING id",
    )
    .bind(kind)
    .bind(source_ref)
    .bind(key)
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn anchor_insert_notifies_and_a_directive_item_insert_stays_silent() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let pool = PgPool::connect(&db.database_url).await?;
    let key = embedder(&pool).await?;
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    item(&pool, &key, "directive", "memory.dossier_entries:1").await?;
    let anchor = item(&pool, &key, "anchor", "mirror:p:1").await?;
    let notice = listener.recv().await?;
    let changed: RowChanged = serde_json::from_str(notice.payload())?;
    assert_eq!(changed.table, RowChangedTable::SemanticAnchors);
    assert_eq!(changed.id, anchor.to_string());
    assert_eq!(changed.session_id, None);
    pool.close().await;
    db.cleanup().await
}

#[tokio::test]
async fn directive_insert_notifies_and_a_promise_insert_stays_silent() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let pool = PgPool::connect(&db.database_url).await?;
    let (session_id, session_uuid) = session(&pool).await?;
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    entry(&pool, session_id, "promise").await?;
    let directive = entry(&pool, session_id, "directive").await?;
    let notice = listener.recv().await?;
    let changed: RowChanged = serde_json::from_str(notice.payload())?;
    assert_eq!(changed.table, RowChangedTable::DossierDirectives);
    assert_eq!(changed.id, directive.to_string());
    assert_eq!(changed.session_id.as_deref(), Some(session_uuid.as_str()));
    pool.close().await;
    db.cleanup().await
}
