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
