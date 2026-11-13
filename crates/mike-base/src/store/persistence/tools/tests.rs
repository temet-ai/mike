use super::*;
use crate::store::persistence::test_support as test_util;
use chrono::Utc;
use serde_json::Value as JsonValue;
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

async fn test_pool() -> anyhow::Result<Option<(PgPool, test_util::TestDbHandle)>> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    Ok(Some((pool, db)))
}

async fn seed_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<(i64, i64)> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects
            (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    let session_fk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_fk))
}

async fn seed_request(pool: &PgPool, session_fk: i64, project_id: i64) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
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
    Ok(id)
}

fn bash_tool() -> JsonValue {
    json!({
        "name": "Bash",
        "description": "Run a shell command",
        "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}},
    })
}

fn read_tool() -> JsonValue {
    json!({
        "name": "Read",
        "description": "Read a file",
        "input_schema": {"type": "object", "properties": {"file_path": {"type": "string"}}},
    })
}

#[test]
fn extract_body_tools_preserves_key_order() -> anyhow::Result<()> {
    let body = br#"{"tools":[{"name":"X","zz":1,"aa":2}]}"#;
    let out = extract_body_tools(body)?;
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].body_wire_bytes,
        br#"{"name":"X","zz":1,"aa":2}"#.to_vec(),
        "extraction must preserve the tool's own key order, not sort it"
    );
    assert_eq!(out[0].tool_name, "X");
    Ok(())
}

#[test]
fn extract_body_tools_preserves_cache_control() -> anyhow::Result<()> {
    let mut with_marker = bash_tool();
    with_marker["cache_control"] = json!({"type": "ephemeral"});
    let body_a = serde_json::to_vec(&json!({"tools": [with_marker]}))?;
    let body_b = serde_json::to_vec(&json!({"tools": [bash_tool()]}))?;

    let a = extract_body_tools(&body_a)?;
    let b = extract_body_tools(&body_b)?;
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_ne!(
        a[0].sha, b[0].sha,
        "cache_control is part of the wire form, so it must change the sha"
    );
    assert!(
        String::from_utf8_lossy(&a[0].body_wire_bytes).contains("cache_control"),
        "cache_control must round-trip in the stored bytes, not be stripped"
    );
    Ok(())
}

#[test]
fn extract_body_tools_missing_tools_array_is_empty() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({"messages": []}))?;
    assert!(extract_body_tools(&body)?.is_empty());
    Ok(())
}

#[test]
fn extract_body_tools_non_array_tools_field_errors() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({"tools": "not-an-array"}))?;
    assert!(extract_body_tools(&body).is_err());
    Ok(())
}

#[tokio::test]
async fn write_request_tools_dedupes_identical_definitions_across_requests() -> anyhow::Result<()> {
    let Some((pool, db)) = test_pool().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) =
        seed_project_session(&pool, "wrt-dedup-proj", &session_id).await?;

    let req1 = seed_request(&pool, session_fk, project_id).await?;
    let req2 = seed_request(&pool, session_fk, project_id).await?;
    let req3 = seed_request(&pool, session_fk, project_id).await?;

    let body_same = serde_json::to_vec(&json!({"tools": [bash_tool(), read_tool()]}))?;
    let mut differing_bash = bash_tool();
    differing_bash["input_schema"]["properties"]["timeout"] = json!({"type": "integer"});
    let body_differing = serde_json::to_vec(&json!({"tools": [differing_bash]}))?;

    let canon1 = extract_body_tools(&body_same)?;
    let canon2 = extract_body_tools(&body_same)?;
    let canon3 = extract_body_tools(&body_differing)?;
    assert_eq!(canon1.len(), 2);
    assert_eq!(canon2.len(), 2);
    assert_eq!(canon3.len(), 1);

    let mut tx = pool.begin().await?;
    write_request_tools(&mut tx, req1, session_fk, &canon1).await?;
    write_request_tools(&mut tx, req2, session_fk, &canon2).await?;
    write_request_tools(&mut tx, req3, session_fk, &canon3).await?;
    tx.commit().await?;

    let total_definitions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tools.definitions")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        total_definitions, 3,
        "Bash + Read shared across req1/req2 (2 rows), plus req3's differing Bash schema (1 more)"
    );

    let (req1_bash_def,): (i64,) = sqlx::query_as(
        "SELECT definition_id FROM requests.tools WHERE request_id = $1 AND tool_name = 'Bash'",
    )
    .bind(req1)
    .fetch_one(&pool)
    .await?;
    let (req2_bash_def,): (i64,) = sqlx::query_as(
        "SELECT definition_id FROM requests.tools WHERE request_id = $1 AND tool_name = 'Bash'",
    )
    .bind(req2)
    .fetch_one(&pool)
    .await?;
    let (req3_bash_def,): (i64,) = sqlx::query_as(
        "SELECT definition_id FROM requests.tools WHERE request_id = $1 AND tool_name = 'Bash'",
    )
    .bind(req3)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        req1_bash_def, req2_bash_def,
        "identical Bash schema across two requests must share one tools.definitions row"
    );
    assert_ne!(
        req1_bash_def, req3_bash_def,
        "a differing Bash schema must resolve to a DIFFERENT tools.definitions row"
    );

    let stored: Vec<u8> = sqlx::query_scalar("SELECT body FROM tools.definitions WHERE id = $1")
        .bind(req1_bash_def)
        .fetch_one(&pool)
        .await?;
    let expected = canon1
        .iter()
        .find(|t| t.tool_name == "Bash")
        .map_or_else(Vec::new, |t| t.body_wire_bytes.clone());
    assert_eq!(
        stored, expected,
        "stored bytes must equal the wire bytes exactly, byte for byte"
    );

    let mut tx2 = pool.begin().await?;
    write_request_tools(&mut tx2, req1, session_fk, &canon1).await?;
    tx2.commit().await?;
    let bridge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM requests.tools WHERE request_id = $1")
            .bind(req1)
            .fetch_one(&pool)
            .await?;
    assert_eq!(bridge_count, 2, "re-run must not duplicate bridge rows");
    let total_after_rerun: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tools.definitions")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        total_after_rerun, 3,
        "re-run must not duplicate definitions either"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_apply_definitions_updates_preexisting_bridge_rows() -> anyhow::Result<()> {
    let Some((pool, db)) = test_pool().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) =
        seed_project_session(&pool, "backfill-tools-proj", &session_id).await?;
    let req = seed_request(&pool, session_fk, project_id).await?;

    sqlx::query!(
        "INSERT INTO requests.tools (request_id, ord, tool_name) VALUES ($1, 0, 'Bash')",
        req,
    )
    .execute(&pool)
    .await?;

    let body = serde_json::to_vec(&json!({"tools": [bash_tool()]}))?;

    let mut tx = pool.begin().await?;
    let updated = backfill_apply_definitions(&mut tx, req, &body).await?;
    tx.commit().await?;
    assert_eq!(updated, 1, "one pre-existing bridge row backfilled");

    let (definition_id,): (Option<i64>,) =
        sqlx::query_as("SELECT definition_id FROM requests.tools WHERE request_id = $1")
            .bind(req)
            .fetch_one(&pool)
            .await?;
    assert!(
        definition_id.is_some(),
        "backfill must populate definition_id"
    );

    let mut tx2 = pool.begin().await?;
    let updated_again = backfill_apply_definitions(&mut tx2, req, &body).await?;
    tx2.commit().await?;
    assert_eq!(
        updated_again, 0,
        "re-run must not re-touch an already-backfilled row"
    );

    db.cleanup().await?;
    Ok(())
}

async fn bridged_definition_ids(pool: &PgPool, request_id: i64) -> anyhow::Result<Vec<i64>> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT definition_id FROM requests.tools WHERE request_id = $1 ORDER BY ord",
    )
    .bind(request_id)
    .fetch_all(pool)
    .await?;
    Ok(ids)
}

#[tokio::test]
async fn write_request_tools_other_session_and_repeated_tool_reuse_stored_definitions()
-> anyhow::Result<()> {
    let Some((pool, db)) = test_pool().await? else {
        return Ok(());
    };
    let (project_a, session_a) =
        seed_project_session(&pool, "wrt-reuse-a", &uuid::Uuid::new_v4().to_string()).await?;
    let (project_b, session_b) =
        seed_project_session(&pool, "wrt-reuse-b", &uuid::Uuid::new_v4().to_string()).await?;
    let first = seed_request(&pool, session_a, project_a).await?;
    let later_same_session = seed_request(&pool, session_a, project_a).await?;
    let other_session = seed_request(&pool, session_b, project_b).await?;

    let body = serde_json::to_vec(&json!({"tools": [bash_tool(), read_tool()]}))?;
    let repeated = serde_json::to_vec(&json!({"tools": [read_tool(), read_tool(), bash_tool()]}))?;

    let mut tx = pool.begin().await?;
    write_request_tools(&mut tx, first, session_a, &extract_body_tools(&body)?).await?;
    write_request_tools(
        &mut tx,
        later_same_session,
        session_a,
        &extract_body_tools(&body)?,
    )
    .await?;
    write_request_tools(
        &mut tx,
        other_session,
        session_b,
        &extract_body_tools(&repeated)?,
    )
    .await?;
    tx.commit().await?;

    let first_ids = bridged_definition_ids(&pool, first).await?;
    let [bash_id, read_id] = first_ids.as_slice() else {
        anyhow::bail!("first request must bridge two tools, got {first_ids:?}");
    };
    assert_eq!(
        bridged_definition_ids(&pool, later_same_session).await?,
        vec![*bash_id, *read_id],
        "a later request in the same session carries the session's definition ids"
    );
    assert_eq!(
        bridged_definition_ids(&pool, other_session).await?,
        vec![*read_id, *read_id, *bash_id],
        "another session reuses stored definitions, and a tool declared twice maps to one row"
    );
    let total_definitions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tools.definitions")
        .fetch_one(&pool)
        .await?;
    assert_eq!(total_definitions, 2, "no definition row is duplicated");

    db.cleanup().await?;
    Ok(())
}
