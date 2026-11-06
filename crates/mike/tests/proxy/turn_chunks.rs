use crate::db_helpers;

use sqlx::PgPool;

async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<(i64, i64)> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_pk))
}

async fn insert_operator_turn(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
    user_prompt: &str,
    final_assistant_text: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let empty_obj = serde_json::json!({});
    let (req_id, _req_ca): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count,
            is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         ) VALUES (
            $1, $2, 2, '/v1/messages', false, 0,
            false, 3, $3,
            $4, $4, 1, 1,
            $5, $5
         ) RETURNING id, created_at",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(user_prompt)
    .bind(now)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO responses.stream_blocks
            (request_id, session_id, seq_ord, content,
             byte_size, content_sha256, arrived_at, block_type_id)
         VALUES ($1, $4, 0, $2, $3, repeat('0', 64), NOW(),
                 (SELECT id FROM responses.stream_block_types WHERE code = 'text'))",
    )
    .bind(req_id)
    .bind(final_assistant_text)
    .bind(final_assistant_text.len() as i64)
    .bind(session_pk)
    .execute(pool)
    .await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id,
            end_request_id,
            outcome_id, classified_at, closed_at,
            user_prompt, final_assistant_text
         ) VALUES ($1, $2, $3, $3,
                   5, NOW(), NOW(),
                   $4, $5)
         RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(req_id)
    .bind(user_prompt)
    .bind(final_assistant_text)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

fn vec_text(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 12 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{x}"));
    }
    s.push(']');
    s
}

async fn insert_chunk(
    pool: &PgPool,
    turn_id: i64,
    chunk_idx: i32,
    chunk_role: &str,
    chunk_text: &str,
    vector: &[f32],
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
             (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         VALUES ($1, $2,
                 (SELECT id FROM memory.chunk_roles WHERE code = $3),
                 $4, $5::vector, 'bge-small-en-v1.5')",
    )
    .bind(turn_id)
    .bind(chunk_idx)
    .bind(chunk_role)
    .bind(chunk_text)
    .bind(vec_text(vector))
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn migration_creates_chunks_table_and_drops_legacy_columns() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
          WHERE table_schema = 'memory' AND table_name = 'operator_turn_chunks'
          ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await?;
    let mut expected = vec![
        "id",
        "turn_id",
        "chunk_idx",
        "chunk_role_id",
        "chunk_text",
        "chunk_embedding",
        "embedding_model",
        "embedded_at",
    ];
    expected.sort();
    let mut got: Vec<&str> = cols.iter().map(String::as_str).collect();
    got.sort();
    assert_eq!(
        got, expected,
        "operator_turn_chunks must expose the Phase 7 schema"
    );

    let legacy_cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
          WHERE table_schema = 'memory'
            AND table_name = 'operator_turns'
            AND column_name IN ('embedding', 'embedding_model')",
    )
    .fetch_all(&pool)
    .await?;
    assert!(
        legacy_cols.is_empty(),
        "Phase 6b columns must have been dropped, found: {legacy_cols:?}"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn long_prompt_is_chunked_to_under_max_chars() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let project_path = "test-long-prompt-chunked";
    let session_id = "12345678-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let (project_id, session_pk) =
        bootstrap_project_session(&pool, project_path, session_id).await?;

    let big_prompt = format!("{}\n\n", "lorem ipsum dolor sit amet. ".repeat(40)).repeat(8);
    assert!(
        big_prompt.chars().count() > 5_000,
        "fixture must be >5000 chars; got {}",
        big_prompt.chars().count()
    );
    let turn_id = insert_operator_turn(&pool, session_pk, project_id, &big_prompt, "ok").await?;

    let chunks = mike::analysis::chunk_text_for_test(&big_prompt, 1500);
    assert!(
        chunks.len() >= 4,
        "expected ≥4 chunks for >5000 char prompt, got {}",
        chunks.len()
    );
    for (i, c) in chunks.iter().enumerate() {
        assert!(
            c.chars().count() <= 1500,
            "chunk {i} exceeds 1500 chars: {}",
            c.chars().count()
        );
    }
    let placeholder_vec = {
        let mut v = vec![0.0f32; 768];
        v[0] = 1.0;
        v
    };
    for (idx, text) in chunks.iter().enumerate() {
        insert_chunk(&pool, turn_id, idx as i32, "prompt", text, &placeholder_vec).await?;
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory.operator_turn_chunks WHERE turn_id = $1")
            .bind(turn_id)
            .fetch_one(&pool)
            .await?;
    assert!(
        count >= 4,
        "expected ≥4 chunk rows persisted for the long prompt, got {count}"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_sql_finds_phrase_only_present_in_late_chunk() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let project_path = "test-late-chunk-recall";
    let session_id = "12345678-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let (project_id, session_pk) =
        bootstrap_project_session(&pool, project_path, session_id).await?;

    let short_prompt = "configure docker compose volumes for postgres";
    let t_short =
        insert_operator_turn(&pool, session_pk, project_id, short_prompt, "did it").await?;

    let filler = "x".repeat(2200);
    let target_phrase = "bisection method on sorted slice";
    let trailing = "y".repeat(3000);
    let long_prompt = format!("{filler} {target_phrase} {trailing}");
    assert!(
        long_prompt.chars().count() > 5_000,
        "long_prompt fixture must exceed 5000 chars"
    );
    let t_long =
        insert_operator_turn(&pool, session_pk, project_id, &long_prompt, "implemented").await?;

    let short_chunks = mike::analysis::chunk_text_for_test(short_prompt, 1500);
    let long_chunks = mike::analysis::chunk_text_for_test(&long_prompt, 1500);
    assert!(
        long_chunks.len() >= 2,
        "long prompt must produce ≥2 chunks; got {}",
        long_chunks.len()
    );

    let target_idx = long_chunks
        .iter()
        .position(|c| c.contains(target_phrase))
        .ok_or_else(|| anyhow::anyhow!("target phrase missing from every chunk; fixture broken"))?;
    assert!(
        target_idx >= 1,
        "target phrase must NOT appear in chunk 0 (would defeat the test)"
    );

    let mut match_vec = vec![0.0f32; 768];
    match_vec[0] = 1.0;
    let mut filler_vec = vec![0.0f32; 768];
    filler_vec[1] = 1.0;

    for (idx, text) in short_chunks.iter().enumerate() {
        insert_chunk(&pool, t_short, idx as i32, "prompt", text, &filler_vec).await?;
    }
    for (idx, text) in long_chunks.iter().enumerate() {
        let v = if idx == target_idx {
            &match_vec
        } else {
            &filler_vec
        };
        insert_chunk(&pool, t_long, idx as i32, "prompt", text, v).await?;
    }

    let mut query_vec = vec![0.0f32; 768];
    query_vec[0] = 0.95;
    query_vec[1] = 0.05;
    let literal = vec_text(&query_vec);

    let rows: Vec<(i64, f32)> = sqlx::query_as(
        "WITH chunk_distances AS (
            SELECT
                c.turn_id,
                MIN(c.chunk_embedding <=> $2::vector) AS min_distance
            FROM memory.operator_turn_chunks c
            JOIN memory.operator_turns t ON t.id = c.turn_id
            JOIN projects.projects p ON p.id = t.project_id
            WHERE p.project_path = $1
            GROUP BY c.turn_id
         )
         SELECT t.id, (1.0 - cd.min_distance)::real AS rank
           FROM chunk_distances cd
           JOIN memory.operator_turns t ON t.id = cd.turn_id
          ORDER BY cd.min_distance ASC, t.created_at DESC
          LIMIT 5",
    )
    .bind(project_path)
    .bind(&literal)
    .fetch_all(&pool)
    .await?;

    assert_eq!(rows.len(), 2, "two ranked turns expected: {rows:?}");
    assert_eq!(
        rows[0].0, t_long,
        "the long-prompt turn must rank first because its LATE chunk matches the query — \
         single-vector embedding (which fastembed truncated at ~512 tokens) could not \
         have surfaced this turn. Got rows: {rows:?}"
    );
    assert_eq!(rows[1].0, t_short, "short turn must come second");
    assert!(
        rows[0].1 > rows[1].1,
        "long turn rank ({}) must exceed short turn rank ({})",
        rows[0].1,
        rows[1].1
    );

    test_db.cleanup().await?;
    Ok(())
}
