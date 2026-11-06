use chrono::Duration;

use super::resolution::{build, facts};
use super::{Fixture, NewTurn, Req, attach, fixture, insert_request, insert_turn};

pub(crate) async fn insert_bash_use(
    f: &Fixture,
    request_id: i64,
    ord: i32,
    wire_id: &str,
    command: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name, input)
         VALUES ($1, $2, $3, $4, 'Bash', jsonb_build_object('command', $5::text))
         RETURNING id",
    )
    .bind(request_id)
    .bind(f.session_id)
    .bind(ord)
    .bind(wire_id)
    .bind(command)
    .fetch_one(f.handle.pool())
    .await?;
    Ok(id)
}

pub(crate) async fn insert_result(
    f: &Fixture,
    request_id: i64,
    tool_use_id: i64,
    is_error: bool,
    content: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tools.results
             (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes)
         VALUES ($1, $2, $3, 0, $4, to_jsonb($5::text), 'sha', 1)",
    )
    .bind(request_id)
    .bind(f.session_id)
    .bind(tool_use_id)
    .bind(is_error)
    .bind(content)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn seed_turn(f: &Fixture) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(9100, 0, "ask")).await?;
    insert_request(f, &Req::lane(9101, "primary", 5)).await?;
    insert_request(f, &Req::lane(9102, "primary", 9)).await?;
    let turn = insert_turn(f, &NewTurn::closed(9100, 9102, 0, 50)).await?;
    attach(f, turn, &[9100, 9101, 9102]).await?;
    Ok(turn)
}

#[tokio::test]
async fn failing_cargo_test_result_is_loaded_with_its_error_flag() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let usage = insert_bash_use(&f, 9101, 0, "toolu_a", "cargo test -p mike").await?;
    insert_result(&f, 9102, usage, true, "Exit code 101\nfailures").await?;
    let uses = f.handle.load_bash_uses(&[turn]).await?;
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].turn_id, turn);
    assert_eq!(uses[0].tool_use_id, "toolu_a");
    assert_eq!(uses[0].command, "cargo test -p mike");
    assert_eq!(uses[0].result_is_error, Some(true));
    assert_eq!(
        uses[0].result_content,
        Some(serde_json::json!("Exit code 101\nfailures"))
    );
    assert_eq!(uses[0].work_item_id, None);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn use_without_result_is_loaded_with_null_result() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    insert_bash_use(&f, 9101, 0, "toolu_a", "scripts/lint-rust").await?;
    let uses = f.handle.load_bash_uses(&[turn]).await?;
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].result_content, None);
    assert_eq!(uses[0].result_is_error, None);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn background_check_reads_registry_row() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    insert_bash_use(&f, 9101, 0, "toolu_bg", "cargo nextest run").await?;
    sqlx::query(
        "INSERT INTO agents.work_items
             (session_id, wire_tool_use_id, kind, task_id, launched_at, state, finished_at, exit_code)
         VALUES ($1, 'toolu_bg', 'bash', 'task-1', $2, 'failed', $3, 3)",
    )
    .bind(f.session_id)
    .bind(f.base)
    .bind(f.base + Duration::seconds(20))
    .execute(f.handle.pool())
    .await?;
    let uses = f.handle.load_bash_uses(&[turn]).await?;
    assert_eq!(uses.len(), 1);
    assert!(uses[0].work_item_id.is_some());
    assert_eq!(uses[0].work_item_state.as_deref(), Some("failed"));
    assert_eq!(uses[0].work_item_exit_code, Some(3));
    assert_eq!(
        uses[0].work_item_finished_at,
        Some(f.base + Duration::seconds(20))
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn load_bash_uses_stays_inside_the_turns_request_range() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    insert_request(&f, &Req::primary(9200, 100, "next")).await?;
    let next = insert_turn(&f, &NewTurn::closed(9200, 9200, 100, 150)).await?;
    attach(&f, next, &[9200]).await?;
    insert_bash_use(&f, 9101, 0, "toolu_in", "cargo test").await?;
    insert_bash_use(&f, 9200, 0, "toolu_out", "cargo test").await?;
    let uses = f.handle.load_bash_uses(&[turn]).await?;
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].tool_use_id, "toolu_in");
    assert!(f.handle.load_bash_uses(&[]).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn failing_cargo_test_result_counts_as_failed_check() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let usage = insert_bash_use(&f, 9101, 0, "toolu_a", "cargo test -p mike").await?;
    insert_result(&f, 9102, usage, true, "Exit code 101\nfailures").await?;
    build(&f).await?;
    let rows = facts(&f).await?;
    let row = rows
        .iter()
        .find(|row| row.operator_turn_id == turn)
        .ok_or_else(|| anyhow::anyhow!("no facts row"))?;
    assert_eq!((row.checks_count, row.checks_failed), (1, 1));
    assert_eq!(row.checks_run[0]["kind"], "test");
    assert_eq!(row.checks_run[0]["exit_code"], 101);
    assert_eq!(row.checks_run[0]["command"], "cargo test -p mike");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn background_check_takes_the_registry_exit_code() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    insert_bash_use(&f, 9101, 0, "toolu_bg", "cargo nextest run").await?;
    sqlx::query(
        "INSERT INTO agents.work_items
             (session_id, wire_tool_use_id, kind, task_id, launched_at, state, finished_at, exit_code)
         VALUES ($1, 'toolu_bg', 'bash', 'task-1', $2, 'failed', $3, 3)",
    )
    .bind(f.session_id)
    .bind(f.base)
    .bind(f.base + Duration::seconds(20))
    .execute(f.handle.pool())
    .await?;
    build(&f).await?;
    let rows = facts(&f).await?;
    let row = rows
        .iter()
        .find(|row| row.operator_turn_id == turn)
        .ok_or_else(|| anyhow::anyhow!("no facts row"))?;
    assert_eq!(row.checks_run[0]["background"], true);
    assert_eq!(row.checks_run[0]["exit_code"], 3);
    assert_eq!(row.checks_failed, 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn rejected_check_is_recorded_with_a_null_exit() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn(&f).await?;
    let usage = insert_bash_use(&f, 9101, 0, "toolu_rej", "scripts/lint-rust").await?;
    insert_result(
        &f,
        9102,
        usage,
        true,
        "The user doesn't want to proceed with this tool use. The tool use was rejected",
    )
    .await?;
    build(&f).await?;
    let rows = facts(&f).await?;
    let row = rows
        .iter()
        .find(|row| row.operator_turn_id == turn)
        .ok_or_else(|| anyhow::anyhow!("no facts row"))?;
    assert_eq!(row.checks_run[0]["rejected"], true);
    assert!(row.checks_run[0]["exit_code"].is_null());
    assert_eq!((row.checks_count, row.checks_failed), (1, 0));
    f.db.cleanup().await?;
    Ok(())
}
