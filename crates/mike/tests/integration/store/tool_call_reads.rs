use mike::store::persistence::PersistHandle;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use mike::store::test_support::TestDb;

#[tokio::test]
async fn request_exists_true_for_seeded_false_for_missing() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    assert!(
        !handle.steerer().request_exists(999_999_999_999).await?,
        "unknown request id must report not-exists"
    );

    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ('/tmp/tool-call-reads', '/tmp/tool-call-reads', NOW(), NOW())
         ON CONFLICT (project_path) DO UPDATE SET last_seen = NOW()
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
               last_user_message, human_prompt, turn_type_id, turn_origin_id
           )
           SELECT $1, $2, rm.id, '/v1/messages', false,
                  1, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id
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

    assert!(
        handle.steerer().request_exists(request_id).await?,
        "seeded request id must report exists"
    );

    handle.pool().close().await;
    Ok(())
}
