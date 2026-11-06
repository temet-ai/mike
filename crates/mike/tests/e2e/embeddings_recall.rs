use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<i64> {
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
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(project_id)
}

async fn insert_operator_turn(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
    user_prompt: &str,
    final_assistant_text: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let empty_obj = serde_json::json!({});
    let req_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count,
            is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         ) SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2,
            (SELECT id FROM requests.methods  WHERE code = 'POST'),
            '/v1/messages', false, 0, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            $3, $4, $4,
            (SELECT id FROM requests.turn_types   WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            $5, $5
         RETURNING id",
    )
    .bind(session_id)
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
         SELECT $1,
            (SELECT id FROM sessions.sessions WHERE session_uuid = $4::text::uuid),
            0, $2, $3, repeat('0', 64), NOW(),
            (SELECT id FROM responses.stream_block_types WHERE code = 'text')",
    )
    .bind(req_id)
    .bind(final_assistant_text)
    .bind(final_assistant_text.len() as i64)
    .bind(session_id)
    .execute(pool)
    .await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id,
            end_request_id,
            outcome_id, closed_at
         ) SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2, $3, $3,
            (SELECT id FROM memory.turn_outcomes WHERE code = 'shipped'),
            NOW()
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(req_id)
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

async fn set_orthogonal_embedding(pool: &PgPool, turn_id: i64, slot: usize) -> anyhow::Result<()> {
    let mut v = vec![0.0f32; 768];
    v[slot] = 1.0;
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
             (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         VALUES ($1, 0,
             (SELECT id FROM memory.chunk_roles WHERE code = 'prompt'),
             'fixture', $2::vector, 'bge-small-en-v1.5')",
    )
    .bind(turn_id)
    .bind(vec_text(&v))
    .execute(pool)
    .await?;
    Ok(())
}

async fn patch_embedding_setting(
    inspect_url: &str,
    key: &str,
    value: serde_json::Value,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .patch(format!("{inspect_url}/api/settings/{key}"))
        .header("user-agent", "mike-isolated-e2e-test")
        .header(
            "x-change-reason",
            "Test lexical fallback against isolated test database",
        )
        .json(&serde_json::json!({ "value": value }))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "failed to patch {key}: {}",
        resp.status()
    );
    Ok(())
}

#[tokio::test]
async fn recall_similar_turns_fts_fallback_with_embedding_disabled() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    patch_embedding_setting(&inspect_url, "embedding.enabled", serde_json::json!(false)).await?;

    let project_path = "test-fts-fallback";
    let session_id = "12345678-1111-4111-8111-111111111111";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    insert_operator_turn(
        &pool,
        session_id,
        project_id,
        "binary search algorithm in Rust",
        "added quicksort to sorting module",
    )
    .await?;
    insert_operator_turn(
        &pool,
        session_id,
        project_id,
        "configure docker compose volumes",
        "updated compose.yaml volumes",
    )
    .await?;

    let client = reqwest::Client::new();
    let url = format!("{}/api/recall/similar-turns", inspect_url);
    let resp = client
        .post(&url)
        .json(&json!({
            "project": project_path,
            "query": "binary search",
            "limit": 5,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    let rows = body["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("turns missing from response: {body}"))?;
    assert!(
        !rows.is_empty(),
        "FTS fallback must return at least one match"
    );
    assert_eq!(
        body["retrieval_path"].as_str(),
        Some("fts"),
        "must use FTS path when embedding is disabled: {body}"
    );
    assert!(
        body["degraded"].as_bool().unwrap_or(false),
        "must be marked degraded when embedding is disabled: {body}"
    );
    let first = &rows[0];
    assert!(
        first["user_prompt"]
            .as_str()
            .unwrap_or("")
            .contains("binary search"),
        "FTS top match must contain 'binary search': {first}"
    );
    let rank = first["rank"].as_f64().unwrap_or(-1.0);
    assert!(rank > 0.0, "FTS rank must be positive: {first}");
    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_similar_turns_embedding_path_ranks_by_cosine_when_vectors_present()
-> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    shutdown.cancel();

    let project_path = "test-embedding-cosine";
    let session_id = "12345678-2222-4222-8222-222222222222";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    let t_binary = insert_operator_turn(
        &pool,
        session_id,
        project_id,
        "binary search algorithm in Rust",
        "implemented bisection over sorted slice",
    )
    .await?;
    let t_sort = insert_operator_turn(
        &pool,
        session_id,
        project_id,
        "sort an array of integers",
        "added quicksort partition routine",
    )
    .await?;
    let t_docker = insert_operator_turn(
        &pool,
        session_id,
        project_id,
        "docker compose volume mount",
        "fixed bind mount in compose.yaml",
    )
    .await?;

    set_orthogonal_embedding(&pool, t_binary, 0).await?;
    set_orthogonal_embedding(&pool, t_sort, 1).await?;
    set_orthogonal_embedding(&pool, t_docker, 2).await?;

    let mut query_vec = vec![0.0f32; 768];
    query_vec[0] = 0.95;
    query_vec[1] = 0.30;
    query_vec[2] = 0.05;
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
    assert_eq!(rows.len(), 3, "three rows must come back: {rows:?}");
    assert_eq!(
        rows[0].0, t_binary,
        "binary-search row must rank first by cosine similarity"
    );
    assert_eq!(
        rows[1].0, t_sort,
        "sort-array row must rank second (slot 1 second-largest)"
    );
    assert_eq!(
        rows[2].0, t_docker,
        "docker row must rank last (slot 2 smallest)"
    );
    assert!(
        rows[0].1 > rows[1].1,
        "binary rank must be higher than sort rank: {rows:?}"
    );
    assert!(
        rows[1].1 > rows[2].1,
        "sort rank must be higher than docker rank: {rows:?}"
    );

    test_db.cleanup().await?;
    Ok(())
}
