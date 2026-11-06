use mike::store::persistence::PersistHandle;

use crate::test_db::TestDb;

async fn insert_request(
    handle: &PersistHandle,
    session_id: i64,
    project_id: i64,
    model: &str,
    age: &str,
) -> anyhow::Result<i64> {
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, model, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', $3, false,
                1, false, rs.id, '{}'::jsonb, 'hello', '',
                rtt.id, rto.id,
                NOW() - $4::interval, NOW() - $4::interval
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(model)
    .bind(age)
    .fetch_one(handle.pool())
    .await?;
    Ok(request_id)
}

async fn insert_request_with_no_model(
    handle: &PersistHandle,
    session_id: i64,
    project_id: i64,
    age: &str,
) -> anyhow::Result<i64> {
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, model, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', NULL, false,
                1, false, rs.id, '{}'::jsonb, 'hello', '',
                rtt.id, rto.id,
                NOW() - $3::interval, NOW() - $3::interval
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(age)
    .fetch_one(handle.pool())
    .await?;
    Ok(request_id)
}

async fn insert_response(
    handle: &PersistHandle,
    request_id: i64,
    response_status: i16,
    token_usage: serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO responses.responses (
            request_id, response_status, token_usage, completed_at
         )
         VALUES ($1, $2, $3, NOW())",
        request_id,
        response_status,
        token_usage,
    )
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn seed_model_usage_fixture(handle: &PersistHandle) -> anyhow::Result<()> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
        "/tmp/model-usage-test",
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

    for (model, age, response_status, token_usage) in [
        (
            "claude-sonnet-5",
            "2 days",
            200,
            serde_json::json!({
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_creation_input_tokens": 10,
                "cache_read_input_tokens": 5,
            }),
        ),
        (
            "claude-sonnet-5",
            "1 day",
            200,
            serde_json::json!({
                "input_tokens": 200,
                "output_tokens": 75,
                "cache_read_input_tokens": 15,
            }),
        ),
        (
            "claude-sonnet-5",
            "400 days",
            200,
            serde_json::json!({
                "input_tokens": 999_999,
                "output_tokens": 999_999,
                "cache_creation_input_tokens": 999_999,
                "cache_read_input_tokens": 999_999,
            }),
        ),
        (
            "claude-haiku-4-5",
            "3 days",
            200,
            serde_json::json!({
                "input_tokens": 30,
                "output_tokens": 10,
                "cache_creation_input_tokens": 1,
                "cache_read_input_tokens": 2,
            }),
        ),
        (
            "claude-sonnet-5",
            "1 day",
            429,
            serde_json::json!({
                "input_tokens": 999_999,
                "output_tokens": 999_999,
                "cache_creation_input_tokens": 999_999,
                "cache_read_input_tokens": 999_999,
            }),
        ),
    ] {
        let request_id = insert_request(handle, session_id, project_id, model, age).await?;
        insert_response(handle, request_id, response_status, token_usage).await?;
    }

    insert_request_with_no_model(handle, session_id, project_id, "2 days").await?;
    insert_request_with_no_model(handle, session_id, project_id, "3 days").await?;
    insert_request_with_no_model(handle, session_id, project_id, "400 days").await?;
    Ok(())
}

#[tokio::test]
async fn list_model_usage_aggregates_by_model_and_honours_window() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    seed_model_usage_fixture(&handle).await?;

    let rows = handle.list_model_usage(7).await?;

    assert_eq!(
        rows.len(),
        3,
        "the two named in-window models plus the unknown-model bucket must appear, got {rows:?}"
    );

    let Some(sonnet) = rows
        .iter()
        .find(|r| r.model.as_deref() == Some("claude-sonnet-5"))
    else {
        panic!("sonnet row must be present, got {rows:?}");
    };
    assert_eq!(
        sonnet.requests, 2,
        "the 400-day-old and the 429 sonnet requests must both be excluded"
    );
    assert_eq!(
        sonnet.input_tokens, 300,
        "100 + 200 across the two in-window, successful rows — the 429 row's \
         999_999 tokens must not be summed in"
    );
    assert_eq!(sonnet.output_tokens, 125, "50 + 75");
    assert_eq!(
        sonnet.cache_creation_tokens, 10,
        "sonnet-2's missing cache_creation key must contribute 0, not NULL the total"
    );
    assert_eq!(sonnet.cache_read_tokens, 20, "5 + 15");

    let Some(haiku) = rows
        .iter()
        .find(|r| r.model.as_deref() == Some("claude-haiku-4-5"))
    else {
        panic!("haiku row must be present, got {rows:?}");
    };
    assert_eq!(haiku.requests, 1);
    assert_eq!(haiku.input_tokens, 30);
    assert_eq!(haiku.output_tokens, 10);
    assert_eq!(haiku.cache_creation_tokens, 1);
    assert_eq!(haiku.cache_read_tokens, 2);

    let Some(unknown) = rows.iter().find(|r| r.model.is_none()) else {
        panic!("unknown-model row must not be dropped, got {rows:?}");
    };
    assert_eq!(
        unknown.requests, 2,
        "the two in-window no-model requests must be counted; the 400-day-old one excluded"
    );
    assert_eq!(
        unknown.input_tokens, 0,
        "no-model requests never got a response, so token sums stay 0 rather than NULL"
    );
    assert_eq!(unknown.output_tokens, 0);
    assert_eq!(unknown.cache_creation_tokens, 0);
    assert_eq!(unknown.cache_read_tokens, 0);
    assert!(
        unknown.last_seen.is_some(),
        "last_seen comes from the request row itself, not the (absent) response"
    );

    let empty = handle.list_model_usage(1).await?;
    assert!(
        empty
            .iter()
            .all(|r| r.model.as_deref() != Some("claude-sonnet-5") || r.requests <= 2),
        "sanity: a 1-day window must not surface the 2-and-3-day-old rows as still-2, {empty:?}"
    );

    handle_db.cleanup().await?;
    Ok(())
}
