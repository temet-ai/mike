use sqlx::PgPool;

use super::materialize_turn_aggregates;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support as test_util;
use crate::store::persistence::workspace::RegisterWorkspaceSession;

struct Seeded {
    session_id: i64,
    project_id: i64,
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct Aggregates {
    tool_call_count: Option<i64>,
    tools_used: Option<Vec<String>>,
    files_touched: Option<Vec<String>>,
    skills_used: Option<Vec<String>>,
    final_assistant_text: Option<String>,
}

async fn seed_session(pool: &PgPool, path: &str) -> anyhow::Result<Seeded> {
    let project_id = test_util::seed_project(pool, path).await?;
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(Seeded {
        session_id,
        project_id,
    })
}

async fn seed_request(pool: &PgPool, s: &Seeded, text: &str) -> anyhow::Result<i64> {
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming, message_count,
             is_tool_loop, status_id, params, last_user_message, human_prompt,
             turn_type_id, turn_origin_id, created_at, completed_at)
         SELECT $1, $2, rm.id, '/v1/messages', true, 1, false, rs.id, '{}'::jsonb, '', '',
                rtt.id, rto.id, NOW(), NOW()
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
          RETURNING id",
    )
    .bind(s.session_id)
    .bind(s.project_id)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO responses.stream_blocks
             (request_id, session_id, seq_ord, content, byte_size, content_sha256, block_type_id)
         SELECT $1, $2, 0, $3, octet_length($3), encode(sha256(convert_to($3, 'UTF8')), 'hex'), bt.id
           FROM responses.stream_block_types bt WHERE bt.code = 'text'",
    )
    .bind(request_id)
    .bind(s.session_id)
    .bind(text)
    .execute(pool)
    .await?;
    Ok(request_id)
}

async fn seed_tool_uses(
    pool: &PgPool,
    s: &Seeded,
    request_id: i64,
    uses: &[(&str, serde_json::Value)],
) -> anyhow::Result<()> {
    let names: Vec<&str> = uses.iter().map(|(name, _)| *name).collect();
    let inputs: Vec<serde_json::Value> = uses.iter().map(|(_, input)| input.clone()).collect();
    sqlx::query(
        "INSERT INTO tools.uses
             (wire_tool_use_id, request_id, ord, tool_name, input, file_path, session_id)
         SELECT 'toolu_' || $1 || '_' || u.ord, $1, u.ord::int, u.name, u.input,
                u.input ->> 'file_path', $2
           FROM unnest($3::text[], $4::jsonb[]) WITH ORDINALITY AS u(name, input, ord)",
    )
    .bind(request_id)
    .bind(s.session_id)
    .bind(&names)
    .bind(&inputs)
    .execute(pool)
    .await?;
    Ok(())
}

async fn open_turn(pool: &PgPool, s: &Seeded, requests: &[i64]) -> anyhow::Result<i64> {
    let (Some(first), Some(last)) = (requests.first(), requests.last()) else {
        anyhow::bail!("a turn needs at least one request");
    };
    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
             (session_id, project_id, start_request_id, end_request_id, outcome_id)
         SELECT $1, $2, $3, $4, tot.id FROM memory.turn_outcomes tot WHERE tot.code = 'unknown'
         RETURNING id",
    )
    .bind(s.session_id)
    .bind(s.project_id)
    .bind(first)
    .bind(last)
    .fetch_one(pool)
    .await?;
    attach(pool, turn_id, requests).await?;
    Ok(turn_id)
}

async fn attach(pool: &PgPool, turn_id: i64, requests: &[i64]) -> anyhow::Result<()> {
    sqlx::query("UPDATE requests.requests SET operator_turn_id = $1 WHERE id = ANY($2)")
        .bind(turn_id)
        .bind(requests)
        .execute(pool)
        .await?;
    Ok(())
}

async fn close_turn(pool: &PgPool, turn_id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE memory.operator_turns SET closed_at = NOW() WHERE id = $1")
        .bind(turn_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn materialize(handle: &PersistHandle, turn_id: i64) -> anyhow::Result<Aggregates> {
    let mut tx = handle.pool.begin().await?;
    materialize_turn_aggregates(&mut tx, turn_id).await?;
    tx.commit().await?;
    read_aggregates(&handle.pool, turn_id).await
}

async fn read_aggregates(pool: &PgPool, turn_id: i64) -> anyhow::Result<Aggregates> {
    Ok(sqlx::query_as(
        "SELECT tool_call_count, tools_used, files_touched, skills_used, final_assistant_text
           FROM memory.operator_turns WHERE id = $1",
    )
    .bind(turn_id)
    .fetch_one(pool)
    .await?)
}

fn strings(values: &[&str]) -> Option<Vec<String>> {
    Some(values.iter().map(|v| (*v).to_string()).collect())
}

async fn seed_two_request_turn(pool: &PgPool, s: &Seeded) -> anyhow::Result<(i64, i64, i64)> {
    let first = seed_request(pool, s, "First. ").await?;
    seed_tool_uses(
        pool,
        s,
        first,
        &[
            ("Read", serde_json::json!({"file_path": "/a"})),
            ("Edit", serde_json::json!({"file_path": "/a"})),
        ],
    )
    .await?;
    let second = seed_request(pool, s, "Second.").await?;
    seed_tool_uses(
        pool,
        s,
        second,
        &[
            ("Read", serde_json::json!({"file_path": "/b"})),
            ("Skill", serde_json::json!({"skill": "fleet"})),
        ],
    )
    .await?;
    let turn = open_turn(pool, s, &[first]).await?;
    Ok((turn, first, second))
}

fn whole_turn() -> Aggregates {
    Aggregates {
        tool_call_count: Some(4),
        tools_used: strings(&["Edit", "Read", "Skill"]),
        files_touched: strings(&["/a", "/b"]),
        skills_used: strings(&["fleet"]),
        final_assistant_text: Some("First. Second.".to_string()),
    }
}

#[tokio::test]
async fn materialize_open_turn_leaves_aggregates_for_close() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let s = seed_session(&handle.pool, "/tmp/turn-aggregates-open").await?;
    let (turn, _, second) = seed_two_request_turn(&handle.pool, &s).await?;
    attach(&handle.pool, turn, &[second]).await?;

    let open = materialize(&handle, turn).await?;
    assert_eq!(open.tool_call_count, None);
    assert_eq!(open.final_assistant_text, None);

    close_turn(&handle.pool, turn).await?;
    assert_eq!(materialize(&handle, turn).await?, whole_turn());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn materialize_late_request_on_closed_turn_equals_whole_turn() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let s = seed_session(&handle.pool, "/tmp/turn-aggregates-late").await?;
    let (turn, _, second) = seed_two_request_turn(&handle.pool, &s).await?;
    close_turn(&handle.pool, turn).await?;
    let first_only = materialize(&handle, turn).await?;
    assert_eq!(first_only.tool_call_count, Some(2));
    assert_eq!(first_only.final_assistant_text.as_deref(), Some("First. "));

    attach(&handle.pool, turn, &[second]).await?;
    assert_eq!(materialize(&handle, turn).await?, whole_turn());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn end_workspace_registration_open_turn_gets_final_text() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let path = "/tmp/turn-aggregates-session-end";
    let session_uuid = uuid::Uuid::new_v4();
    handle
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid,
            project_path: Some(path),
            cwd: path,
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    let (session_id, project_id): (i64, i64) =
        sqlx::query_as("SELECT id, project_id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session_uuid)
            .fetch_one(&handle.pool)
            .await?;
    let s = Seeded {
        session_id,
        project_id,
    };
    let request = seed_request(&handle.pool, &s, "All done.").await?;
    let turn = open_turn(&handle.pool, &s, &[request]).await?;

    handle
        .end_workspace_registration(session_uuid, None)
        .await?;

    let closed = read_aggregates(&handle.pool, turn).await?;
    assert_eq!(closed.final_assistant_text.as_deref(), Some("All done."));
    assert_eq!(closed.tool_call_count, Some(0));
    db.cleanup().await?;
    Ok(())
}
