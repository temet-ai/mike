use anyhow::Context;
use chrono::Utc;
use mike::store::persistence::reconstruct::{backfill_envelope_batch, strip_bulk_arrays};
use mike::store::persistence::rehydrate::fetch_request_body_by_id;
use mike::store::persistence::{messages, tools};
use mike::store::test_support::TestDb;
use serde_json::Value as JsonValue;
use sqlx::PgPool;

const WIRE_BODY: &[u8] = br#"{"model":"claude-opus-5","messages":[{"role":"user","content":[{"type":"text","text":"first"}]}],"max_tokens":64000,"stream":true}"#;

const ENVELOPE_WIRE_BODY: &[u8] = br#"{"model":"claude-opus-5","messages":[{"role":"user","content":[{"type":"text","text":"first"}]},{"role":"assistant","content":[{"type":"text","text":"second"}]},{"role":"user","content":[{"type":"text","text":"third","cache_control":{"type":"ephemeral","ttl":"1h"}}]}],"system":[{"type":"text","text":"you are a helpful assistant"}],"tools":[{"name":"Bash","description":"run a command","input_schema":{"type":"object","properties":{"command":{"type":"string"}}}},{"name":"Read","description":"read a file","input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}],"metadata":{"user_id":"u-1"},"max_tokens":64000,"thinking":{"type":"adaptive"},"context_management":{"edits":[{"type":"clear_thinking_20251015","keep":"all"}]},"output_config":{"effort":"xhigh"},"diagnostics":{"previous_message_id":"msg_01"},"stream":true}"#;

async fn shared_pool() -> anyhow::Result<Option<PgPool>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let pool = PgPool::connect(&db.database_url).await?;
    Ok(Some(pool))
}

async fn seed_request_row(pool: &PgPool) -> anyhow::Result<i64> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(format!("issue68-reconstruct-{}", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await?;
    let session_fk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, $3
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_fk)
    .bind(project_id)
    .bind(Utc::now())
    .fetch_one(pool)
    .await?;
    Ok(request_id)
}

#[tokio::test]
async fn an_intact_body_is_served_verbatim() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };
    let request_id = seed_request_row(&pool).await?;
    sqlx::query("UPDATE requests.requests SET request_body = $2 WHERE id = $1")
        .bind(request_id)
        .bind(WIRE_BODY)
        .execute(&pool)
        .await?;

    let served = fetch_request_body_by_id(&pool, request_id as u64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("row has a body"))?;
    assert_eq!(
        served.bytes.as_ref(),
        WIRE_BODY,
        "an uncompacted row must come back byte-identical, envelope or not"
    );
    assert!(
        !served.reconstructed,
        "a row served from its own captured bytes is not a rebuild"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn a_row_with_no_body_and_no_envelope_serves_nothing() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };
    let request_id = seed_request_row(&pool).await?;
    let served = fetch_request_body_by_id(&pool, request_id as u64).await?;
    assert!(served.is_none(), "nothing captured, nothing served");

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn a_cleared_body_rebuilds_from_its_envelope() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };
    let request_id = seed_request_row(&pool).await?;

    let canonical = messages::canonicalise_body_messages(ENVELOPE_WIRE_BODY)?;
    assert_eq!(canonical.len(), 3, "three messages canonicalised");
    let wire_tools = tools::extract_body_tools(ENVELOPE_WIRE_BODY)?;
    assert_eq!(wire_tools.len(), 2, "two tools extracted");

    let mut tx = pool.begin().await?;
    messages::write_request_messages(
        &mut tx,
        request_id,
        &canonical,
        &std::collections::HashMap::new(),
    )
    .await?;
    let session_fk: i64 =
        sqlx::query_scalar("SELECT session_id FROM requests.requests WHERE id = $1")
            .bind(request_id)
            .fetch_one(&mut *tx)
            .await?;
    tools::write_request_tools(&mut tx, request_id, session_fk, &wire_tools).await?;
    tx.commit().await?;

    let envelope = strip_bulk_arrays(ENVELOPE_WIRE_BODY).context("envelope captured")?;
    let compact_sql = "UPDATE requests.requests \
         SET request_envelope = $2, request_body = NULL, request_body_size = $3 \
         WHERE id = $1";
    sqlx::query(compact_sql)
        .bind(request_id)
        .bind(&envelope)
        .bind(ENVELOPE_WIRE_BODY.len() as i32)
        .execute(&pool)
        .await?;

    let served = fetch_request_body_by_id(&pool, request_id as u64)
        .await?
        .context("a compacted row must still serve a body")?;
    assert!(
        served.reconstructed,
        "a cleared row must announce that its body was rebuilt"
    );

    let original: JsonValue = serde_json::from_slice(ENVELOPE_WIRE_BODY)?;
    let rebuilt: JsonValue = serde_json::from_slice(&served.bytes)?;

    assert_eq!(
        rebuilt.get("messages"),
        original.get("messages"),
        "every message, in order, with its cache_control breakpoint back in place"
    );
    assert_eq!(
        rebuilt, original,
        "the whole body must carry the same JSON value it was captured with"
    );

    for key in [
        "metadata",
        "context_management",
        "output_config",
        "diagnostics",
        "stream",
        "system",
        "max_tokens",
        "thinking",
        "model",
    ] {
        assert_eq!(
            rebuilt.get(key),
            original.get(key),
            "`{key}` lives only in the envelope and must survive verbatim"
        );
    }

    let served_text = std::str::from_utf8(&served.bytes)?;
    let original_text = std::str::from_utf8(ENVELOPE_WIRE_BODY)?;
    let tools_at = original_text
        .find(r#""tools":["#)
        .context("the fixture declares tools")?;
    assert!(
        served_text.ends_with(&original_text[tools_at..]),
        "tools and every field after them must rebuild byte-for-byte"
    );
    assert!(
        served_text.starts_with(r#"{"model":"claude-opus-5","messages":["#),
        "the envelope's head must be the original's head, byte for byte"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn the_backfill_recovers_discarded_cache_control_placement() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };
    let request_id = seed_request_row(&pool).await?;

    let canonical = messages::canonicalise_body_messages(ENVELOPE_WIRE_BODY)?;
    let mut tx = pool.begin().await?;
    messages::write_request_messages(
        &mut tx,
        request_id,
        &canonical,
        &std::collections::HashMap::new(),
    )
    .await?;
    tx.commit().await?;
    let blank = "UPDATE requests.messages SET cache_control = NULL WHERE request_id = $1";
    let blanked = sqlx::query(blank)
        .bind(request_id)
        .execute(&pool)
        .await?
        .rows_affected();
    assert_eq!(blanked, 3, "three messages seeded");

    sqlx::query("UPDATE requests.requests SET request_body = $2 WHERE id = $1")
        .bind(request_id)
        .bind(ENVELOPE_WIRE_BODY)
        .execute(&pool)
        .await?;

    let batch = backfill_envelope_batch(&pool, request_id, Some(request_id), 10)
        .await?
        .context("the seeded row is a candidate")?;
    assert_eq!(batch.updated, 1, "its envelope is captured");
    assert_eq!(
        batch.cache_control_recovered, 1,
        "the one message carrying a breakpoint gets it back"
    );

    let recovered: Option<JsonValue> = sqlx::query_scalar(
        "SELECT cache_control FROM requests.messages WHERE request_id = $1 AND ord = 2",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    let recovered = recovered.context("ord 2 carried the breakpoint on the wire")?;
    assert_eq!(
        recovered
            .pointer("/0/value/ttl")
            .and_then(JsonValue::as_str),
        Some("1h"),
        "recovered verbatim, not approximated"
    );
    assert_eq!(
        recovered.pointer("/0/pointer").and_then(JsonValue::as_str),
        Some("/content/0/cache_control"),
        "and at the position it actually occupied"
    );

    let again = backfill_envelope_batch(&pool, request_id, Some(request_id), 10).await?;
    assert!(
        again.is_none(),
        "a row that already has an envelope is no longer a candidate"
    );

    pool.close().await;
    Ok(())
}
