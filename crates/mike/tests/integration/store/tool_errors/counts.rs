use super::fixtures::{seed_project, seed_request, seed_session};
use super::isolated_handle;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::tool_uses::{ToolUseGroup, ToolUsesFilter};
use mike::types::SortOrder;

const SESSION: &str = "d0000000-0000-4000-8000-000000000386";

async fn seed_mixed_uses(handle: &PersistHandle) -> anyhow::Result<()> {
    let project_id = seed_project(handle.pool(), "proj-tool-uses-counts").await?;
    let session_id = seed_session(handle.pool(), SESSION, project_id).await?;
    let request_id = seed_request(handle.pool(), session_id, project_id).await?;
    sqlx::query(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         SELECT 'tu-count-' || ord, $1, ord, tool_name, input::jsonb, $2
           FROM (VALUES
             (1, 'Bash', '{\"command\": \"gh pr create --fill\"}'),
             (2, 'Bash', '{\"command\": \"ls -la\"}'),
             (3, 'mcp__mike__list_sessions', '{\"limit\": 5}'),
             (4, 'mcp__mike__get_session', '{\"session_id\": \"x\"}'),
             (5, 'mcp__tether__inbox', '{}'),
             (6, 'Read', '{\"file_path\": \"/tmp/100%_done\"}')
           ) AS v(ord, tool_name, input)",
    )
    .bind(request_id)
    .bind(session_id)
    .execute(handle.pool())
    .await?;
    Ok(())
}

fn filter() -> ToolUsesFilter {
    ToolUsesFilter {
        project: None,
        tool_name: None,
        tool_name_prefix: None,
        input_contains: None,
        session_id: Some(SESSION.to_owned()),
        since: None,
        until: None,
        errors_only: false,
        limit: 50,
        offset: 0,
        cursor: None,
        order: SortOrder::Asc,
    }
}

fn pairs(groups: Vec<mike::store::persistence::tool_uses::ToolUseCount>) -> Vec<(String, i64)> {
    groups.into_iter().map(|g| (g.key, g.count)).collect()
}

#[tokio::test]
async fn tool_uses_prefix_and_input_filters_match_only_their_rows() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };
    seed_mixed_uses(&handle).await?;

    let mut by_prefix = filter();
    by_prefix.tool_name_prefix = Some("mcp__mike__".to_owned());
    let rows = handle.launch().list_tool_uses(&by_prefix).await?.rows;
    let names: Vec<String> = rows.into_iter().map(|r| r.tool_name).collect();
    assert_eq!(
        names,
        vec!["mcp__mike__list_sessions", "mcp__mike__get_session"]
    );

    let mut gh_pr = filter();
    gh_pr.tool_name = Some("Bash".to_owned());
    gh_pr.input_contains = Some("GH PR".to_owned());
    let rows = handle.launch().list_tool_uses(&gh_pr).await?.rows;
    let ids: Vec<String> = rows.into_iter().map(|r| r.wire_tool_use_id).collect();
    assert_eq!(ids, vec!["tu-count-1"]);

    let mut literal_percent = filter();
    literal_percent.input_contains = Some("100%_".to_owned());
    let rows = handle.launch().list_tool_uses(&literal_percent).await?.rows;
    let ids: Vec<String> = rows.into_iter().map(|r| r.wire_tool_use_id).collect();
    assert_eq!(ids, vec!["tu-count-6"]);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn tool_uses_counts_group_by_tool_server_and_session() -> anyhow::Result<()> {
    let Some((handle, db)) = isolated_handle().await? else {
        return Ok(());
    };
    seed_mixed_uses(&handle).await?;

    let by_server = handle
        .launch()
        .count_tool_uses_by(&filter(), ToolUseGroup::Server)
        .await?;
    assert_eq!(
        pairs(by_server),
        vec![
            ("built-in".to_owned(), 3),
            ("mike".to_owned(), 2),
            ("tether".to_owned(), 1),
        ]
    );

    let mut mike_only = filter();
    mike_only.tool_name_prefix = Some("mcp__mike__".to_owned());
    let by_tool = handle
        .launch()
        .count_tool_uses_by(&mike_only, ToolUseGroup::Tool)
        .await?;
    assert_eq!(
        pairs(by_tool),
        vec![
            ("mcp__mike__get_session".to_owned(), 1),
            ("mcp__mike__list_sessions".to_owned(), 1),
        ]
    );

    let by_session = handle
        .launch()
        .count_tool_uses_by(&filter(), ToolUseGroup::Session)
        .await?;
    assert_eq!(pairs(by_session), vec![(SESSION.to_owned(), 6)]);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
