use mike::store::persistence::PersistHandle;
use mike::store::test_support::TestDb;

async fn seed_request(pool: &sqlx::PgPool) -> anyhow::Result<(i64, i64)> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ('/tmp/orphan-test', '/tmp/orphan-test', NOW(), NOW())
         RETURNING id"
    )
    .fetch_one(pool)
    .await?;

    let session_db_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, NOW(), NOW())
         RETURNING id",
        project_id,
    )
    .fetch_one(pool)
    .await?;

    let request_id: i64 = sqlx::query_scalar!(
        r#"INSERT INTO requests.requests (
               session_id, project_id, method_id, path, is_streaming,
               message_count, is_tool_loop, status_id, params,
               last_user_message, human_prompt, turn_type_id, turn_origin_id
           )
           SELECT $1, $2, rm.id, '/v1/messages', false,
                  1, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
           WHERE rm.code = 'POST' AND rs.code = 'complete'
             AND rtt.code = 'user_prompt' AND rto.code = 'primary'
           RETURNING id"#,
        session_db_id,
        project_id,
    )
    .fetch_one(pool)
    .await?;

    Ok((session_db_id, request_id))
}

async fn seed_message(pool: &sqlx::PgPool, sha: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar!(
        "INSERT INTO messages.messages (message_sha256, body, byte_size, role_id, first_seen, last_seen)
         VALUES ($1, '{}'::jsonb, 2, 1, NOW(), NOW())
         RETURNING id",
        sha,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn isolated_handle()
-> anyhow::Result<Option<(PersistHandle, mike::store::test_support::TestDbHandle)>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((handle, db)))
}

#[tokio::test]
async fn sweep_orphaned_messages_removes_unreferenced_keeps_referenced() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };

    let orphan_id = seed_message(handle.pool(), "orphan-sha").await?;
    let referenced_id = seed_message(handle.pool(), "referenced-sha").await?;
    let (_session_db_id, request_id) = seed_request(handle.pool()).await?;
    sqlx::query!(
        "INSERT INTO requests.messages (request_id, message_id, ord) VALUES ($1, $2, 0)",
        request_id,
        referenced_id,
    )
    .execute(handle.pool())
    .await?;

    let removed = handle.sweep_orphaned_messages(100).await?;
    assert_eq!(
        removed, 1,
        "exactly the one orphan must be swept — this test needs an otherwise-empty \
         messages table, so it keeps acquire_isolated"
    );

    let orphan_still_present: Option<i64> =
        sqlx::query_scalar!("SELECT id FROM messages.messages WHERE id = $1", orphan_id)
            .fetch_optional(handle.pool())
            .await?;
    assert!(
        orphan_still_present.is_none(),
        "the unreferenced row must be deleted"
    );

    let referenced_still_present: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM messages.messages WHERE id = $1",
        referenced_id
    )
    .fetch_optional(handle.pool())
    .await?;
    assert!(
        referenced_still_present.is_some(),
        "the row referenced by requests.messages must survive"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sweep_orphaned_messages_keeps_row_referenced_only_via_deltas() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };

    let delta_referenced_id = seed_message(handle.pool(), "delta-only-sha").await?;
    let (session_db_id, request_id) = seed_request(handle.pool()).await?;
    sqlx::query!(
        "INSERT INTO requests.deltas (
             request_id, session_id, message_id, change_kind_id, ord
         )
         SELECT $1, $2, $3, dck.id, 0
         FROM requests.delta_change_kinds dck
         WHERE dck.code = 'added'",
        request_id,
        session_db_id,
        delta_referenced_id,
    )
    .execute(handle.pool())
    .await?;

    let removed = handle.sweep_orphaned_messages(100).await?;
    assert_eq!(
        removed, 0,
        "a row referenced only via requests.deltas must not be swept"
    );

    let still_present: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM messages.messages WHERE id = $1",
        delta_referenced_id
    )
    .fetch_optional(handle.pool())
    .await?;
    assert!(
        still_present.is_some(),
        "the row referenced by requests.deltas must survive"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn count_orphaned_messages_matches_sweep_target() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };

    seed_message(handle.pool(), "count-orphan-sha").await?;

    let before = handle.count_orphaned_messages().await?;
    assert_eq!(before.count, 1);
    assert!(before.total_bytes > 0);

    handle.sweep_orphaned_messages(100).await?;

    let after = handle.count_orphaned_messages().await?;
    assert_eq!(after.count, 0);
    assert_eq!(after.total_bytes, 0);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
