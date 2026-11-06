use crate::test_db;

use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

async fn insert_request_and_tool_uses(
    pool: &PgPool,
    session_id: &str,
    tool_calls: &Value,
) -> anyhow::Result<i64> {
    let project_path = format!("test-project-{session_id}");
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, $2, $2
         WHERE NOT EXISTS (SELECT 1 FROM projects.projects WHERE project_path = $1)
         RETURNING id",
    )
    .bind(&project_path)
    .bind(now)
    .fetch_optional(pool)
    .await?
    .unwrap_or(0i64);
    let project_id: i64 = if project_id == 0 {
        sqlx::query_scalar("SELECT id FROM projects.projects WHERE project_path = $1")
            .bind(&project_path)
            .fetch_one(pool)
            .await?
    } else {
        project_id
    };

    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;

    let empty_obj = json!({});
    let session_row_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(session_id)
            .fetch_one(pool)
            .await?;

    let (request_id, _request_created_at): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            $1, $2, 2, '/v1/messages', false, 0, false,
            3, $3,
            '', '', 1, 1,
            $4
         ) RETURNING id, created_at",
    )
    .bind(session_row_id)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(now)
    .fetch_one(pool)
    .await?;

    if let Some(arr) = tool_calls.as_array() {
        for (ord, call) in arr.iter().enumerate() {
            let tool_id = call
                .get("tool_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("tool_calls[{ord}].tool_id missing"))?;
            let name = call
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let input_json = call
                .get("input_json")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let input_value: Option<Value> = if input_json.is_empty() {
                None
            } else {
                serde_json::from_str(input_json).ok()
            };
            sqlx::query(
                "INSERT INTO tools.uses
                    (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (wire_tool_use_id) DO NOTHING",
            )
            .bind(tool_id)
            .bind(request_id)
            .bind(ord as i32)
            .bind(name)
            .bind(input_value.as_ref())
            .bind(session_row_id)
            .execute(pool)
            .await?;
        }
    }

    Ok(request_id)
}

#[tokio::test]
async fn tool_uses_table_holds_one_row_per_tool_call_with_full_input() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let edit_input = json!({"file_path": "/a", "old_string": "foo", "new_string": "bar"});
    let bash_input = json!({"command": "x", "description": "exec x"});
    let write_input = json!({"file_path": "/b", "content": "hello"});
    let tool_calls = json!([
        {"tool_id": "toolu_edit_001",  "name": "Edit",  "input_json": serde_json::to_string(&edit_input)?,  "complete": true},
        {"tool_id": "toolu_bash_002",  "name": "Bash",  "input_json": serde_json::to_string(&bash_input)?,  "complete": true},
        {"tool_id": "toolu_write_003", "name": "Write", "input_json": serde_json::to_string(&write_input)?, "complete": true},
    ]);
    let session_id_str = uuid::Uuid::new_v4().to_string();
    let request_id = insert_request_and_tool_uses(&pool, &session_id_str, &tool_calls).await?;

    type ToolUseRow = (i64, String, String, String, Option<String>, i32, Value);
    let rows: Vec<ToolUseRow> = sqlx::query_as(
        "SELECT tu.request_id, s.session_uuid::text AS session_id, tu.wire_tool_use_id, tu.tool_name,
                CASE WHEN tu.tool_name IN ('Read','Write','Edit')
                     THEN tu.input->>'file_path' ELSE NULL
                END AS file_path,
                tu.ord,
                tu.input
           FROM tools.uses tu
           JOIN requests.requests r
             ON r.id = tu.request_id
           JOIN sessions.sessions s
             ON s.id = r.session_id
          WHERE tu.request_id = $1
          ORDER BY tu.ord ASC",
    )
    .bind(request_id)
    .fetch_all(&pool)
    .await?;

    assert_eq!(rows.len(), 3, "tool_uses table holds one row per tool call");

    assert_eq!(rows[0].0, request_id);
    assert_eq!(rows[0].1, session_id_str);
    assert_eq!(rows[0].2, "toolu_edit_001");
    assert_eq!(rows[0].3, "Edit");
    assert_eq!(rows[0].4.as_deref(), Some("/a"));
    assert_eq!(rows[0].5, 0, "ord starts at 0 (FK table is 0-indexed)");
    assert_eq!(rows[0].6, edit_input, "full input JSONB, not sliced");

    assert_eq!(rows[1].2, "toolu_bash_002");
    assert_eq!(rows[1].3, "Bash");
    assert!(rows[1].4.is_none(), "Bash has no file_path");
    assert_eq!(rows[1].5, 1);
    assert_eq!(rows[1].6, bash_input);

    assert_eq!(rows[2].2, "toolu_write_003");
    assert_eq!(rows[2].3, "Write");
    assert_eq!(rows[2].4.as_deref(), Some("/b"));
    assert_eq!(rows[2].5, 2);
    assert_eq!(rows[2].6, write_input);

    test_db.cleanup().await?;
    Ok(())
}
