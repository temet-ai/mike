use super::fixtures::{seed_project, seed_request, seed_session};
use super::isolated_handle;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::keyset::KeysetCursor;
use mike::store::persistence::tool_uses::ToolUsesFilter;
use mike::types::SortOrder;

const SESSION: &str = "d0000000-0000-4000-8000-000000000001";

async fn seed_uses(handle: &PersistHandle) -> anyhow::Result<()> {
    let project_id = seed_project(handle.pool(), "proj-tool-uses-paging").await?;
    let session_id = seed_session(handle.pool(), SESSION, project_id).await?;
    let request_id = seed_request(handle.pool(), session_id, project_id).await?;
    sqlx::query!(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id,
                                 created_at)
         SELECT 'tu-page-' || n, $1, n, 'Read', '{}'::jsonb, $2,
                now() - make_interval(mins => 10 - n)
           FROM generate_series(1, 5) AS g(n)",
        request_id,
        session_id,
    )
    .execute(handle.pool())
    .await?;
    Ok(())
}

fn filter(order: SortOrder, cursor: Option<KeysetCursor>, offset: i64) -> ToolUsesFilter {
    ToolUsesFilter {
        project: None,
        tool_name: None,
        session_id: Some(SESSION.to_owned()),
        since: None,
        errors_only: false,
        limit: 2,
        offset,
        cursor,
        order,
    }
}

async fn walk(handle: &PersistHandle, order: SortOrder) -> anyhow::Result<Vec<Vec<String>>> {
    let mut pages = Vec::new();
    let mut cursor = None;
    loop {
        let page = handle.list_tool_uses(&filter(order, cursor, 0)).await?;
        assert_eq!(page.total.count, 5);
        assert!(!page.total.capped);
        pages.push(page.rows.into_iter().map(|r| r.wire_tool_use_id).collect());
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return Ok(pages),
        }
    }
}

#[tokio::test]
async fn list_tool_uses_cursor_walks_every_row_once_in_both_orders() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };
    seed_uses(&handle).await?;

    assert_eq!(
        walk(&handle, SortOrder::Desc).await?,
        vec![
            vec!["tu-page-5", "tu-page-4"],
            vec!["tu-page-3", "tu-page-2"],
            vec!["tu-page-1"],
        ]
    );
    assert_eq!(
        walk(&handle, SortOrder::Asc).await?,
        vec![
            vec!["tu-page-1", "tu-page-2"],
            vec!["tu-page-3", "tu-page-4"],
            vec!["tu-page-5"],
        ]
    );

    let by_offset = handle
        .list_tool_uses(&filter(SortOrder::Desc, None, 2))
        .await?;
    let ids: Vec<String> = by_offset
        .rows
        .into_iter()
        .map(|r| r.wire_tool_use_id)
        .collect();
    assert_eq!(ids, vec!["tu-page-3", "tu-page-2"]);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_tool_uses_unparseable_session_matches_nothing() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };
    seed_uses(&handle).await?;

    let mut unknown = filter(SortOrder::Desc, None, 0);
    unknown.session_id = Some("not-a-uuid".to_owned());
    let page = handle.list_tool_uses(&unknown).await?;
    assert!(page.rows.is_empty());
    assert_eq!(page.total.count, 0);
    assert_eq!(page.next_cursor, None);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
