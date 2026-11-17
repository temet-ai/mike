use super::fixtures::{seed_project, seed_request, seed_session};
use super::isolated_handle;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::file_writes::{self, FileWriteFilter};
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::listings::{ListingWindow, RequestListFilters};
use mike::store::persistence::tool_errors::ToolErrorFilter;
use mike::types::SortOrder;
use uuid::Uuid;

const SESSION_A: &str = "e0000000-0000-4000-8000-00000000000a";
const SESSION_B: &str = "e0000000-0000-4000-8000-00000000000b";

struct Seeded {
    request_a: i64,
    request_b: i64,
}

async fn seed_write_and_error(
    handle: &PersistHandle,
    session_db_id: i64,
    request_id: i64,
    label: &str,
) -> anyhow::Result<()> {
    let tool_use_id: i64 = sqlx::query_scalar!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id,
                                 operation, file_path)
         VALUES ($1, $2, 0, 'Edit', '{}'::jsonb, $3, 'write', '/shared/session-filter.rs')
         RETURNING id",
        format!("tu-session-filter-{label}"),
        request_id,
        session_db_id,
    )
    .fetch_one(handle.pool())
    .await?;
    sqlx::query!(
        "INSERT INTO tools.results
                (request_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes,
                 session_id)
         VALUES ($1, $2, 0, true, '\"err\"'::jsonb, $3, 3, $4)",
        request_id,
        tool_use_id,
        label.repeat(64),
        session_db_id,
    )
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn seed_two_sessions(handle: &PersistHandle) -> anyhow::Result<Seeded> {
    let project_id = seed_project(handle.pool(), "proj-session-filter").await?;
    let session_a = seed_session(handle.pool(), SESSION_A, project_id).await?;
    let session_b = seed_session(handle.pool(), SESSION_B, project_id).await?;
    let request_a = seed_request(handle.pool(), session_a, project_id).await?;
    let request_b = seed_request(handle.pool(), session_b, project_id).await?;
    seed_write_and_error(handle, session_a, request_a, "a").await?;
    seed_write_and_error(handle, session_b, request_b, "b").await?;
    Ok(Seeded {
        request_a,
        request_b,
    })
}

fn error_filter(session_uuid: Option<Uuid>) -> ToolErrorFilter {
    ToolErrorFilter {
        project: None,
        tool_name: None,
        session_uuid,
        since: None,
        until: None,
        limit: 100,
        offset: 0,
        cursor: None,
        order: SortOrder::Desc,
    }
}

#[tokio::test]
async fn list_tool_errors_session_filter_drops_other_sessions_in_count_and_page()
-> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };
    let seeded = seed_two_sessions(&handle).await?;

    let both = handle.list_tool_errors(&error_filter(None)).await?;
    assert_eq!(both.rows.len(), 2, "unfiltered feed holds both sessions");

    let only_a = handle
        .list_tool_errors(&error_filter(Some(Uuid::parse_str(SESSION_A)?)))
        .await?;
    assert_eq!(only_a.rows.len(), 1);
    assert_eq!(only_a.total.count, 1, "count query applies the filter too");
    assert_eq!(only_a.rows[0].session_id, SESSION_A);
    assert_eq!(only_a.rows[0].request_id, seeded.request_a);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn file_writes_session_filter_drops_other_sessions_in_count_and_page() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };
    seed_two_sessions(&handle).await?;
    let filter = |session_uuid| FileWriteFilter {
        project: Some("proj-session-filter"),
        from: None,
        to: None,
        path: None,
        session_uuid,
        collisions_only: false,
    };

    let unfiltered = filter(None);
    let summaries = file_writes::summarise_file_writes(handle.pool(), &unfiltered, 0, 10).await?;
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].session_count, 2);

    let scoped = filter(Some(Uuid::parse_str(SESSION_B)?));
    assert_eq!(
        file_writes::count_file_writes(handle.pool(), &scoped).await?,
        1
    );
    let summaries = file_writes::summarise_file_writes(handle.pool(), &scoped, 0, 10).await?;
    assert_eq!(summaries[0].session_count, 1);
    assert_eq!(summaries[0].write_count, 1);
    let paths = vec![summaries[0].file_path.clone()];
    let writes = file_writes::fetch_writes_for_paths(handle.pool(), &paths, &scoped).await?;
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].session_uuid, SESSION_B);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_requests_session_filter_drops_other_sessions() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };
    let seeded = seed_two_sessions(&handle).await?;
    let window = ListingWindow {
        order: SortOrder::Desc,
        date_from: None,
        date_to: None,
    };
    let filters = RequestListFilters {
        prompt_sha256: None,
        session_uuid: Some(Uuid::parse_str(SESSION_A)?),
    };

    let (rows, _) = handle
        .launch()
        .list_requests_paginated(0, 50, &filters, &window, None)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].session_id, SESSION_A);
    assert_eq!(i64::try_from(rows[0].id)?, seeded.request_a);
    assert_ne!(i64::try_from(rows[0].id)?, seeded.request_b);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
