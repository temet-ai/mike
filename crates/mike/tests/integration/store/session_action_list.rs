use mike::store::persistence::PersistHandle;
use mike::store::test_support::{self, TestDb};

async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(handle))
}

fn unique(label: &str) -> String {
    format!("{label}-{}", uuid::Uuid::new_v4().simple())
}

async fn seed_session_with_calls(
    label: &str,
    calls: &[(&str, Option<serde_json::Value>)],
) -> anyhow::Result<Option<(PersistHandle, i64, Vec<i64>)>> {
    let Some(handle) = shared_handle().await? else {
        return Ok(None);
    };

    let project_id = test_support::seed_project(handle.pool(), &unique(label)).await?;
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let other_uuid = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(handle.pool(), &session_uuid, &other_uuid, project_id).await?;
    test_support::seed_session_turns(handle.pool(), &session_uuid, project_id, 1).await?;

    let (session_db_id, request_id): (i64, i64) = sqlx::query_as(
        "SELECT s.id, r.id
           FROM sessions.sessions s
           JOIN requests.requests r ON r.session_id = s.id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session_uuid)
    .fetch_one(handle.pool())
    .await?;

    let mut tool_use_ids = Vec::with_capacity(calls.len());
    for (i, (name, input)) in calls.iter().enumerate() {
        let file_path = input
            .as_ref()
            .and_then(|v| v.get("file_path"))
            .and_then(|v| v.as_str());
        let tool_use_id: i64 = sqlx::query_scalar(
            "INSERT INTO tools.uses
                (wire_tool_use_id, request_id, session_id, ord, tool_name, input, file_path)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id",
        )
        .bind(format!("toolu_test_{session_uuid}_{i}"))
        .bind(request_id)
        .bind(session_db_id)
        .bind(i as i32)
        .bind(name)
        .bind(input)
        .bind(file_path)
        .fetch_one(handle.pool())
        .await?;
        tool_use_ids.push(tool_use_id);
    }

    Ok(Some((handle, session_db_id, tool_use_ids)))
}

#[tokio::test]
async fn fetch_session_action_list_empty_for_unknown_session() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let rows = handle.fetch_session_action_list(-1).await?;

    assert!(rows.is_empty());
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_session_action_list_returns_every_row_unbounded() -> anyhow::Result<()> {
    let calls: Vec<(&str, Option<serde_json::Value>)> = (0..40)
        .map(|i| {
            (
                "Read",
                Some(serde_json::json!({"file_path": format!("/src/file_{i}.rs")})),
            )
        })
        .collect();
    let Some((handle, session_db_id, _tool_use_ids)) =
        seed_session_with_calls("w1-158-unbounded", &calls).await?
    else {
        return Ok(());
    };

    let rows = handle.fetch_session_action_list(session_db_id).await?;

    assert_eq!(rows.len(), 40, "every call returned, no row-count cap");
    assert_eq!(rows[0].file_path.as_deref(), Some("/src/file_0.rs"));
    assert_eq!(rows[39].file_path.as_deref(), Some("/src/file_39.rs"));
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_session_action_list_projects_bash_command_only_for_bash() -> anyhow::Result<()> {
    let calls: Vec<(&str, Option<serde_json::Value>)> = vec![
        (
            "Bash",
            Some(serde_json::json!({"command": "cargo nextest run -p mike"})),
        ),
        (
            "Edit",
            Some(serde_json::json!({"file_path": "/src/lib.rs"})),
        ),
    ];
    let Some((handle, session_db_id, _tool_use_ids)) =
        seed_session_with_calls("w1-158-bash-projection", &calls).await?
    else {
        return Ok(());
    };

    let rows = handle.fetch_session_action_list(session_db_id).await?;

    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].command.as_deref(),
        Some("cargo nextest run -p mike")
    );
    assert_eq!(rows[0].file_path, None, "Bash rows carry no file_path");
    assert_eq!(rows[1].command, None, "non-Bash rows carry no command");
    assert_eq!(rows[1].file_path.as_deref(), Some("/src/lib.rs"));
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_session_action_list_carries_error_outcome() -> anyhow::Result<()> {
    let calls: Vec<(&str, Option<serde_json::Value>)> =
        vec![("Bash", Some(serde_json::json!({"command": "false"})))];
    let Some((handle, session_db_id, tool_use_ids)) =
        seed_session_with_calls("w1-158-error-outcome", &calls).await?
    else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO tools.results
            (request_id, tool_use_id, session_id, ord, is_error, content, content_sha256, content_bytes)
         SELECT tu.request_id, tu.id, tu.session_id, 0, true, $2::jsonb, 'deadbeef', 13
           FROM tools.uses tu
          WHERE tu.id = $1",
    )
    .bind(tool_use_ids[0])
    .bind(serde_json::json!("exit status 1"))
    .execute(handle.pool())
    .await?;

    let rows = handle.fetch_session_action_list(session_db_id).await?;

    assert_eq!(rows.len(), 1);
    assert!(rows[0].has_result);
    assert_eq!(rows[0].result_is_error, Some(true));
    handle.pool().close().await;
    Ok(())
}
