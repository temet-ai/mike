use crate::db_helpers;

use serde_json::Value;
use sqlx::PgPool;

use super::fixtures::{EditSeed, ToolCallSeed, insert_edit, insert_tool_call};

async fn seed_line_delta_fixture(
    pool: &PgPool,
    project: &str,
    session: &str,
    reverted: &str,
    grew: &str,
    whole: &str,
) -> anyhow::Result<()> {
    insert_edit(
        pool,
        EditSeed {
            project_path: project,
            session_id: session,
            file_path: reverted,
            wire_id: "toolu_ld_r1",
            old_string: "one\n",
            new_string: "one\ntwo\nthree\n",
            offset_ms: 100,
        },
    )
    .await?;
    insert_edit(
        pool,
        EditSeed {
            project_path: project,
            session_id: session,
            file_path: reverted,
            wire_id: "toolu_ld_r2",
            old_string: "one\ntwo\nthree\n",
            new_string: "one\n",
            offset_ms: 200,
        },
    )
    .await?;

    insert_edit(
        pool,
        EditSeed {
            project_path: project,
            session_id: session,
            file_path: grew,
            wire_id: "toolu_ld_g1",
            old_string: "a\n",
            new_string: "a\nb\nc\nd\n",
            offset_ms: 300,
        },
    )
    .await?;

    insert_tool_call(
        pool,
        ToolCallSeed {
            project_path: project,
            session_id: session,
            tool_name: "Write",
            operation: "write",
            file_path: whole,
            wire_id: "toolu_ld_w1",
            offset_ms: 400,
        },
    )
    .await?;
    Ok(())
}

fn find_file_row(files: &[Value], path: &str, body: &Value) -> Value {
    files
        .iter()
        .find(|f| f["file_path"].as_str() == Some(path))
        .unwrap_or_else(|| panic!("{path} missing from {body}"))
        .clone()
}

fn assert_reverted_file_nets_to_zero(row: &Value) {
    assert_eq!(
        row["write_count"].as_i64(),
        Some(2),
        "the count still reports both calls — that is the number that misleads: {row}"
    );
    assert_eq!(
        row["net_line_delta"].as_i64(),
        Some(0),
        "an edit and its revert must cancel, which is the whole point: {row}"
    );
}

fn assert_grown_file_reports_net_delta(row: &Value) -> anyhow::Result<()> {
    assert_eq!(row["write_count"].as_i64(), Some(1));
    assert_eq!(
        row["net_line_delta"].as_i64(),
        Some(3),
        "one line became four, so three were added: {row}"
    );
    let writes = row["writes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("writes must be an array"))?;
    assert_eq!(
        writes[0]["line_delta"].as_i64(),
        Some(3),
        "the per-write delta must agree with the file's total: {row}"
    );
    Ok(())
}

fn assert_whole_file_write_has_no_delta(row: &Value) {
    assert_eq!(row["write_count"].as_i64(), Some(1));
    assert!(
        row.get("net_line_delta").is_none(),
        "a Write has no before-state, so the field must be absent rather than 0 — \
         reporting 0 would claim no change was made: {row}"
    );
}

#[tokio::test]
async fn file_writes_reports_a_net_line_delta_that_a_revert_cancels() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let project = "line-delta-fixture-project";
    let session = "c0000000-0000-4000-8000-000000000c01";
    let reverted = "/repo/src/reverted.rs";
    let grew = "/repo/src/grew.rs";
    let whole = "/repo/src/whole.rs";

    seed_line_delta_fixture(&pool, project, session, reverted, grew, whole).await?;

    let client = reqwest::Client::new();
    let body: Value = client
        .get(format!("{inspect_url}/api/file-writes?project={project}"))
        .send()
        .await?
        .json()
        .await?;

    let files = body["files"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("files must be an array"))?;

    assert_reverted_file_nets_to_zero(&find_file_row(files, reverted, &body));
    assert_grown_file_reports_net_delta(&find_file_row(files, grew, &body))?;
    assert_whole_file_write_has_no_delta(&find_file_row(files, whole, &body));

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
