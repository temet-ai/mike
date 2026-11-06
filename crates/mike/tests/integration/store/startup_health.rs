use mike::store::persistence::PersistHandle;
use mike::store::test_support::TestDb;

#[tokio::test]
async fn startup_row_counts_reflects_seeded_rows() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let zero = handle.startup_row_counts().await?;
    assert_eq!(
        zero.requests, 0,
        "startup_row_counts scans every row with no per-test scoping, so this test needs an \
         otherwise-empty database, hence acquire_isolated"
    );
    assert_eq!(zero.sessions, 0);
    assert_eq!(zero.tool_uses, 0);

    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ('/tmp/startup-health', '/tmp/startup-health', NOW(), NOW())
         RETURNING id"
    )
    .fetch_one(handle.pool())
    .await?;
    let session_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, NOW(), NOW())
         RETURNING id",
        project_id,
    )
    .fetch_one(handle.pool())
    .await?;
    let request_id: i64 = sqlx::query_scalar!(
        r#"INSERT INTO requests.requests (
               session_id, project_id, method_id, path, is_streaming,
               message_count, is_tool_loop, status_id, params,
               last_user_message, human_prompt, turn_type_id, turn_origin_id,
               completed_at
           )
           SELECT $1, $2, rm.id, '/v1/messages', false,
                  1, true, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, NOW()
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
           WHERE rm.code = 'POST' AND rs.code = 'complete'
             AND rtt.code = 'user_prompt' AND rto.code = 'primary'
           RETURNING id"#,
        session_id,
        project_id,
    )
    .fetch_one(handle.pool())
    .await?;
    sqlx::query!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, session_id)
         VALUES ('toolu_count_001', $1, 0, 'Bash', $2)",
        request_id,
        session_id,
    )
    .execute(handle.pool())
    .await?;

    let counts = handle.startup_row_counts().await?;
    assert_eq!(counts.requests, 1, "one request row seeded");
    assert_eq!(counts.sessions, 1, "one session row seeded");
    assert_eq!(counts.tool_uses, 1, "one tool_use row seeded");

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
