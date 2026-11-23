use std::time::Duration;

use chrono::Utc;
use serde_json::Value;
use sqlx::postgres::PgListener;

use super::fixture;
use super::seed::{ToolUse, project, request, session, tool_use};

async fn tables_until_marker(
    listener: &mut PgListener,
    pool: &sqlx::PgPool,
    project_id: i64,
) -> anyhow::Result<Vec<String>> {
    let marker = session(pool, project_id, None).await?;
    sqlx::query("UPDATE sessions.sessions SET title = 'marker' WHERE id = $1")
        .bind(marker)
        .execute(pool)
        .await?;
    let mut tables = Vec::new();
    loop {
        let notification = tokio::time::timeout(Duration::from_secs(5), listener.recv())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for the marker notice"))??;
        let payload: Value = serde_json::from_str(notification.payload())?;
        if payload["table"] == "sessions" {
            return Ok(tables);
        }
        if let Some(table) = payload["table"].as_str() {
            tables.push(table.to_owned());
        }
    }
}

#[tokio::test]
async fn checkout_edits_notice_on_an_edit_to_an_absolute_path_and_not_on_other_tools()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let worker = session(pool, project_id, Some("/work/alpha")).await?;
    let req = request(pool, worker, project_id, Utc::now()).await?;
    let mut listener = PgListener::connect_with(pool).await?;
    listener.listen("proxy.row_changed").await?;

    for (name, path) in [
        ("Read", Some("/work/beta/src/lib.rs")),
        ("Write", Some("relative/lib.rs")),
        ("Bash", None),
    ] {
        let call = ToolUse {
            session_id: worker,
            request_id: req,
            name,
            file_path: path,
            at: Utc::now(),
        };
        tool_use(pool, &call).await?;
    }
    let quiet = tables_until_marker(&mut listener, pool, project_id).await?;
    assert!(!quiet.iter().any(|t| t == "checkout_edits"), "{quiet:?}");

    for name in ["Edit", "Write"] {
        let call = ToolUse {
            session_id: worker,
            request_id: req,
            name,
            file_path: Some("/work/beta/src/lib.rs"),
            at: Utc::now(),
        };
        tool_use(pool, &call).await?;
    }
    let edits = tables_until_marker(&mut listener, pool, project_id).await?;
    assert_eq!(
        edits.iter().filter(|t| *t == "checkout_edits").count(),
        2,
        "{edits:?}"
    );
    drop(listener);
    f.db.cleanup().await
}
