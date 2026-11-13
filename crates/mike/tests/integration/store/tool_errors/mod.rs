use mike::store::persistence::launch_repo::LaunchPersist as _;
mod fixtures;
mod paging;

use fixtures::{seed_project, seed_request, seed_session};

use mike::store::persistence::PersistHandle;
use mike::store::persistence::tool_errors::ToolErrorFilter;
use mike::store::persistence::tool_uses::{ToolUsesFeedRow, ToolUsesFilter};
use mike::store::test_support::TestDb;
use mike::types::SortOrder;

async fn isolated_handle()
-> anyhow::Result<Option<(PersistHandle, mike::store::test_support::TestDbHandle)>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((handle, db)))
}

#[tokio::test]
async fn list_tool_errors_deduped_to_one_per_tool_use() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };

    let project_id = seed_project(handle.pool(), "proj-dedup-test").await?;
    let session_id = seed_session(
        handle.pool(),
        "b0000000-0000-4000-8000-000000000001",
        project_id,
    )
    .await?;

    let req1 = seed_request(handle.pool(), session_id, project_id).await?;
    let req2 = seed_request(handle.pool(), session_id, project_id).await?;
    let req3 = seed_request(handle.pool(), session_id, project_id).await?;

    let tool_use_id: i64 = sqlx::query_scalar!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             SELECT $1, $2, 0, 'Bash', '{}'::jsonb, r.session_id
             FROM requests.requests r WHERE r.id = $2
             RETURNING id",
        "tu-dedup-test",
        req1,
    )
    .fetch_one(handle.pool())
    .await?;

    for (carrying_req, sha) in [(req1, "aaa"), (req2, "bbb"), (req3, "ccc")] {
        sqlx::query!(
            "INSERT INTO tools.results
                    (request_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes,
                     session_id)
                 VALUES ($1, $2, 0, true, '\"err\"'::jsonb, $3, 3, $4)
                 ON CONFLICT (tool_use_id) DO NOTHING",
            carrying_req,
            tool_use_id,
            sha.repeat(64 / sha.len()),
            session_id,
        )
        .execute(handle.pool())
        .await?;
    }

    let filter = ToolErrorFilter {
        project: None,
        tool_name: None,
        since: None,
        until: None,
        limit: 100,
        offset: 0,
        cursor: None,
        order: SortOrder::Desc,
    };
    let page = handle.list_tool_errors(&filter).await?;
    let (total_count, rows) = (page.total.count, page.rows);

    assert_eq!(
        rows.len(),
        1,
        "three result rows for one tool_use → one deduped row"
    );
    assert_eq!(total_count, 1, "count must equal the deduped row count");
    assert_eq!(
        rows[0].result_id,
        sqlx::query_scalar::<_, i64>("SELECT MIN(id) FROM tools.results WHERE tool_use_id = $1")
            .bind(tool_use_id)
            .fetch_one(handle.pool())
            .await?,
        "result_id must be the MIN(id) — earliest result row"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_tool_errors_count_matches_rows_under_cross_project_filter() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };

    let project_a = seed_project(handle.pool(), "proj-A-emit").await?;
    let project_b = seed_project(handle.pool(), "proj-B-carry").await?;

    let session_a = seed_session(
        handle.pool(),
        "a0000000-0000-4000-8000-000000000010",
        project_a,
    )
    .await?;
    let session_b = seed_session(
        handle.pool(),
        "a0000000-0000-4000-8000-000000000011",
        project_b,
    )
    .await?;

    let emitting_req = seed_request(handle.pool(), session_a, project_a).await?;
    let carrying_req = seed_request(handle.pool(), session_b, project_b).await?;

    let tool_use_id: i64 = sqlx::query_scalar!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             SELECT $1, $2, 0, 'Edit', '{}'::jsonb, r.session_id
             FROM requests.requests r WHERE r.id = $2
             RETURNING id",
        "tu-cross-project",
        emitting_req,
    )
    .fetch_one(handle.pool())
    .await?;

    sqlx::query!(
        "INSERT INTO tools.results
                (request_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes,
                 session_id)
             VALUES ($1, $2, 0, true, '\"err\"'::jsonb, $3, 3, $4)",
        carrying_req,
        tool_use_id,
        "a".repeat(64),
        session_b,
    )
    .execute(handle.pool())
    .await?;

    let filter = ToolErrorFilter {
        project: Some("proj-A-emit".to_string()),
        tool_name: None,
        since: None,
        until: None,
        limit: 100,
        offset: 0,
        cursor: None,
        order: SortOrder::Desc,
    };
    let page = handle.list_tool_errors(&filter).await?;
    let (total_count, rows) = (page.total.count, page.rows);
    assert_eq!(
        rows.len(),
        1,
        "page query must surface the error under the emitting-project filter"
    );
    assert_eq!(
        total_count,
        rows.len() as i64,
        "total_count must equal rows.len() — COUNT and page must scope to the same project"
    );
    assert_eq!(total_count, 1, "exactly one error attributed to project A");

    let filter_b = ToolErrorFilter {
        project: Some("proj-B-carry".to_string()),
        tool_name: None,
        since: None,
        until: None,
        limit: 100,
        offset: 0,
        cursor: None,
        order: SortOrder::Desc,
    };
    let page_b = handle.list_tool_errors(&filter_b).await?;
    let (count_b, rows_b) = (page_b.total.count, page_b.rows);
    assert_eq!(
        rows_b.len(),
        0,
        "no error is emitter-attributed to project B"
    );
    assert_eq!(
        count_b, 0,
        "COUNT must also be zero for project B — consistent with the empty page"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

struct ToolUseStateFixture {
    result_success_id: i64,
    result_failed_id: i64,
    error_content: serde_json::Value,
}

async fn seed_clean_tool_use(
    pool: &sqlx::PgPool,
    session_id: i64,
    req_clean: i64,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             VALUES ($1, $2, 0, 'Bash', '{}'::jsonb, $3)",
        "tu-clean",
        req_clean,
        session_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_successful_tool_use(
    pool: &sqlx::PgPool,
    session_id: i64,
    req_success: i64,
) -> anyhow::Result<i64> {
    let tu_success_id: i64 = sqlx::query_scalar!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             VALUES ($1, $2, 0, 'Read', '{}'::jsonb, $3) RETURNING id",
        "tu-success",
        req_success,
        session_id,
    )
    .fetch_one(pool)
    .await?;
    let result_success_id: i64 = sqlx::query_scalar!(
        "INSERT INTO tools.results
                (request_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes,
                 session_id)
             VALUES ($1, $2, 0, false, '\"ok\"'::jsonb, $3, 2, $4) RETURNING id",
        req_success,
        tu_success_id,
        "s".repeat(64),
        session_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(result_success_id)
}

async fn seed_failed_tool_use(
    pool: &sqlx::PgPool,
    session_id: i64,
    req_failed: i64,
) -> anyhow::Result<(i64, serde_json::Value)> {
    let tu_failed_id: i64 = sqlx::query_scalar!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             VALUES ($1, $2, 0, 'Edit', '{}'::jsonb, $3) RETURNING id",
        "tu-failed",
        req_failed,
        session_id,
    )
    .fetch_one(pool)
    .await?;
    let error_content = serde_json::json!({"error": "boom"});
    let result_failed_id: i64 = sqlx::query_scalar!(
        "INSERT INTO tools.results
                (request_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes,
                 session_id)
             VALUES ($1, $2, 0, true, $3, $4, 9, $5) RETURNING id",
        req_failed,
        tu_failed_id,
        &error_content,
        "e".repeat(64),
        session_id,
    )
    .fetch_one(pool)
    .await?;
    Ok((result_failed_id, error_content))
}

async fn seed_tool_use_state_matrix(
    pool: &sqlx::PgPool,
    session_id: i64,
    req_clean: i64,
    req_success: i64,
    req_failed: i64,
) -> anyhow::Result<ToolUseStateFixture> {
    seed_clean_tool_use(pool, session_id, req_clean).await?;
    let result_success_id = seed_successful_tool_use(pool, session_id, req_success).await?;
    let (result_failed_id, error_content) =
        seed_failed_tool_use(pool, session_id, req_failed).await?;

    Ok(ToolUseStateFixture {
        result_success_id,
        result_failed_id,
        error_content,
    })
}

fn assert_clean_row_has_no_result(row: &ToolUsesFeedRow) {
    assert!(!row.is_error, "no result captured -> is_error = false");
    assert_eq!(row.result_id, None, "no result captured -> result_id null");
    assert_eq!(row.error_content, None);
    assert_eq!(row.error_content_bytes, None);
    assert_eq!(row.error_content_sha256, None);
}

fn assert_success_row_has_result_but_no_error_content(
    row: &ToolUsesFeedRow,
    result_success_id: i64,
) {
    assert!(!row.is_error, "successful result -> is_error = false");
    assert_eq!(
        row.result_id,
        Some(result_success_id),
        "result_id is populated for ANY captured result, error or not"
    );
    assert_eq!(
        row.error_content, None,
        "a non-errored result's content is never carried on this row"
    );
    assert_eq!(row.error_content_bytes, None);
    assert_eq!(row.error_content_sha256, None);
}

fn assert_failed_row_carries_full_error_content(
    row: &ToolUsesFeedRow,
    fixture: &ToolUseStateFixture,
) {
    assert!(row.is_error);
    assert_eq!(row.result_id, Some(fixture.result_failed_id));
    assert_eq!(
        row.error_content,
        Some(fixture.error_content.clone()),
        "error_content is the FULL blob verbatim"
    );
    assert_eq!(row.error_content_bytes, Some(9));
    assert_eq!(row.error_content_sha256, Some("e".repeat(64)));
}

fn assert_unfiltered_feed_reports_three_state_matrix(
    rows: &[ToolUsesFeedRow],
    fixture: &ToolUseStateFixture,
) {
    assert_eq!(rows.len(), 3);

    let by_name = |name: &str| {
        rows.iter()
            .find(|r| r.wire_tool_use_id == name)
            .unwrap_or_else(|| panic!("row {name} missing from unfiltered feed"))
    };

    assert_clean_row_has_no_result(by_name("tu-clean"));
    assert_success_row_has_result_but_no_error_content(
        by_name("tu-success"),
        fixture.result_success_id,
    );
    assert_failed_row_carries_full_error_content(by_name("tu-failed"), fixture);
}

async fn assert_errors_only_scopes_to_the_failed_tool_use(
    handle: &PersistHandle,
    base_filter: ToolUsesFilter,
    fixture: &ToolUseStateFixture,
) -> anyhow::Result<()> {
    let errors_only_filter = ToolUsesFilter {
        errors_only: true,
        ..base_filter
    };
    let page = handle.launch().list_tool_uses(&errors_only_filter).await?;
    let errors_rows = page.rows;
    assert_eq!(
        page.total.count, 1,
        "COUNT must scope to errored tool uses under errors_only"
    );
    assert_eq!(errors_rows.len(), 1);
    assert_eq!(errors_rows[0].wire_tool_use_id, "tu-failed");
    assert!(errors_rows[0].is_error);
    assert_eq!(errors_rows[0].result_id, Some(fixture.result_failed_id));
    assert_eq!(
        errors_rows[0].error_content,
        Some(fixture.error_content.clone())
    );
    Ok(())
}

#[tokio::test]
async fn list_tool_uses_joins_error_columns_and_scopes_errors_only() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };

    let project_id = seed_project(handle.pool(), "proj-tool-uses-errors-only").await?;
    let session_id = seed_session(
        handle.pool(),
        "c0000000-0000-4000-8000-000000000001",
        project_id,
    )
    .await?;

    let req_clean = seed_request(handle.pool(), session_id, project_id).await?;
    let req_success = seed_request(handle.pool(), session_id, project_id).await?;
    let req_failed = seed_request(handle.pool(), session_id, project_id).await?;

    let fixture = seed_tool_use_state_matrix(
        handle.pool(),
        session_id,
        req_clean,
        req_success,
        req_failed,
    )
    .await?;

    let filter = ToolUsesFilter {
        project: None,
        tool_name: None,
        session_id: None,
        since: None,
        errors_only: false,
        limit: 100,
        offset: 0,
        cursor: None,
        order: SortOrder::Desc,
    };
    let page = handle.launch().list_tool_uses(&filter).await?;
    assert_eq!(page.total.count, 3, "all three tool uses are counted");
    assert!(!page.total.capped);
    assert_eq!(page.next_cursor, None);
    assert_unfiltered_feed_reports_three_state_matrix(&page.rows, &fixture);

    assert_errors_only_scopes_to_the_failed_tool_use(&handle, filter, &fixture).await?;

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
