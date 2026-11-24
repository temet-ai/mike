use super::PersistHandle;
use super::message_repair::{find_message_copies, repair_message_copies};
use super::reconstruct::rehydrate_body;
use super::request_write::IdMode;
use super::test_support as test_util;
use crate::store::models::RequestId;

struct Seeded {
    handle: PersistHandle,
    db: test_util::TestDbHandle,
    session_db_id: i64,
    later: i64,
}

async fn seed_with_a_copy(first_id: u64) -> anyhow::Result<Option<Seeded>> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4().to_string();
    let body = serde_json::json!({"messages": [
        {"role": "user", "content": "opening"},
        {"role": "assistant", "content": "reply"}
    ]})
    .to_string();
    for (n, id) in [first_id, first_id + 1].into_iter().enumerate() {
        let mut req =
            crate::store::tests::make_test_request(Some(RequestId::from_raw(id)), &session);
        req.created_at = chrono::DateTime::from_timestamp(1_790_000_000 + i64::try_from(n)?, 0)
            .unwrap_or_default();
        req.request_body = Some(bytes::Bytes::from(body.clone()));
        handle
            .insert_request_initial(&req, n == 0, IdMode::Ensure)
            .await?;
    }
    let later = i64::try_from(first_id + 1)?;
    sqlx::query(
        "WITH copy AS (
             INSERT INTO messages.messages (message_sha256, body, byte_size, role_id)
             SELECT m.message_sha256, m.body, m.byte_size, m.role_id
               FROM requests.messages rm JOIN messages.messages m ON m.id = rm.message_id
              WHERE rm.request_id = $1 AND rm.ord = 0
             RETURNING id)
         UPDATE requests.messages rm SET message_id = copy.id
           FROM copy WHERE rm.request_id = $1 AND rm.ord = 0",
    )
    .bind(later)
    .execute(&handle.pool)
    .await?;
    let session_db_id: i64 =
        sqlx::query_scalar("SELECT session_id FROM requests.requests WHERE id = $1")
            .bind(later)
            .fetch_one(&handle.pool)
            .await?;
    idle(&handle, session_db_id, "2 hours").await?;
    Ok(Some(Seeded {
        handle,
        db,
        session_db_id,
        later,
    }))
}

async fn idle(handle: &PersistHandle, session_db_id: i64, ago: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE sessions.sessions SET last_seen = now() - $2::interval WHERE id = $1")
        .bind(session_db_id)
        .bind(ago)
        .execute(&handle.pool)
        .await?;
    Ok(())
}

async fn stored(handle: &PersistHandle) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM messages.messages")
        .fetch_one(&handle.pool)
        .await?)
}

async fn rebuilt(handle: &PersistHandle, id: i64) -> anyhow::Result<Option<bytes::Bytes>> {
    let (body, envelope): (Option<Vec<u8>>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT request_body, request_envelope FROM requests.requests WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&handle.pool)
    .await?;
    Ok(rehydrate_body(
        &handle.pool,
        id,
        body.map(bytes::Bytes::from),
        envelope.map(bytes::Bytes::from),
    )
    .await?
    .map(|served| served.bytes))
}

#[tokio::test]
async fn find_message_copies_seeded_copy_is_found_once() -> anyhow::Result<()> {
    let Some(seeded) = seed_with_a_copy(99_101).await? else {
        return Ok(());
    };
    let copies = find_message_copies(&seeded.handle.pool).await?;
    assert_eq!(copies.len(), 1);
    assert!(copies[0].copy_id > copies[0].keep_id);
    seeded.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn repair_message_copies_idle_copy_is_removed_and_every_body_kept() -> anyhow::Result<()> {
    let Some(seeded) = seed_with_a_copy(99_201).await? else {
        return Ok(());
    };
    let before = rebuilt(&seeded.handle, seeded.later).await?;
    let copies = find_message_copies(&seeded.handle.pool).await?;
    let repair = repair_message_copies(&seeded.handle.pool, &copies, 60).await?;
    assert_eq!(repair.repaired, 1);
    assert_eq!(repair.references_moved, 1);
    assert_eq!(repair.rows_deleted, 1);
    assert!(repair.requests_checked >= 1);
    assert!(repair.mismatched_requests.is_empty());
    assert_eq!(stored(&seeded.handle).await?, 2);
    assert_eq!(rebuilt(&seeded.handle, seeded.later).await?, before);
    assert!(find_message_copies(&seeded.handle.pool).await?.is_empty());
    seeded.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn repair_message_copies_same_hash_different_body_is_left_alone() -> anyhow::Result<()> {
    let Some(seeded) = seed_with_a_copy(99_301).await? else {
        return Ok(());
    };
    sqlx::query(
        "UPDATE messages.messages m SET body = jsonb_set(m.body, '{content}', '\"tampered\"')
           FROM requests.messages rm
          WHERE rm.request_id = $1 AND rm.ord = 0 AND m.id = rm.message_id",
    )
    .bind(seeded.later)
    .execute(&seeded.handle.pool)
    .await?;
    let copies = find_message_copies(&seeded.handle.pool).await?;
    let repair = repair_message_copies(&seeded.handle.pool, &copies, 60).await?;
    assert_eq!(repair.repaired, 0);
    assert_eq!(stored(&seeded.handle).await?, 3);
    seeded.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn repair_message_copies_copy_of_an_active_session_is_left_alone() -> anyhow::Result<()> {
    let Some(seeded) = seed_with_a_copy(99_401).await? else {
        return Ok(());
    };
    idle(&seeded.handle, seeded.session_db_id, "1 minute").await?;
    let copies = find_message_copies(&seeded.handle.pool).await?;
    let repair = repair_message_copies(&seeded.handle.pool, &copies, 60).await?;
    assert_eq!(repair.repaired, 0);
    assert_eq!(stored(&seeded.handle).await?, 3);
    seeded.db.cleanup().await?;
    Ok(())
}
