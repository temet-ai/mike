use chrono::Duration;
use mike::dossier::entries::run_entries_step;
use mike::store::persistence::dossier_repo::DossierPersist as _;

use super::entries;
use crate::turn_facts::{
    Fixture, NewTurn, Req, attach, fixture, insert_request, insert_turn, sample_row,
};

const WAIT_TEXT: &str =
    "The suite is running in the background. I'll report back when it finishes.";

async fn promise_turn(f: &Fixture) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(9100, 0, "run the suite")).await?;
    sqlx::query("UPDATE requests.requests SET completed_at = $2 WHERE id = $1")
        .bind(9100_i64)
        .bind(f.base + Duration::seconds(30))
        .execute(f.handle.pool())
        .await?;
    let turn = insert_turn(
        f,
        &NewTurn {
            final_text: Some(WAIT_TEXT),
            ..NewTurn::closed(9100, 9100, 0, 40)
        },
    )
    .await?;
    attach(f, turn, &[9100]).await?;
    let mut row = sample_row(turn, f.session_id, f.base);
    row.ended_on = "promise_or_wait".to_string();
    row.stop_request_id = Some(9100);
    f.handle.dossier().upsert_turn_facts(&[row]).await?;
    Ok(turn)
}

async fn work_item(f: &Fixture, state: &str, finished_secs: Option<i64>) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO agents.work_items
             (session_id, wire_tool_use_id, kind, task_id, launched_at, state, finished_at)
         VALUES ($1, 'toolu_' || gen_random_uuid(), 'bash', 'task-1', $2, $3, $4)
         RETURNING id",
    )
    .bind(f.session_id)
    .bind(f.base + Duration::seconds(10))
    .bind(state)
    .bind(finished_secs.map(|s| f.base + Duration::seconds(s)))
    .fetch_one(f.handle.pool())
    .await?)
}

async fn set_finished(f: &Fixture, id: i64, state: &str, secs: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE agents.work_items SET state = $2, finished_at = $3 WHERE id = $1")
        .bind(id)
        .bind(state)
        .bind(f.base + Duration::seconds(secs))
        .execute(f.handle.pool())
        .await?;
    Ok(())
}

async fn step(f: &Fixture) -> anyhow::Result<()> {
    run_entries_step(&f.handle, &[f.session_id], Duration::minutes(30)).await?;
    Ok(())
}

#[tokio::test]
async fn a_promise_opens_with_its_running_backing_and_breaks_when_the_item_fails()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let item = work_item(&f, "running", None).await?;
    promise_turn(&f).await?;
    step(&f).await?;
    let open = entries(&f).await?;
    assert_eq!(open.len(), 1, "rows: {open:?}");
    assert_eq!(open[0].status, "open");
    assert_eq!(open[0].data["registry_coverage"], serde_json::json!(true));
    assert_eq!(
        open[0].data["backing"][0]["work_item_id"],
        serde_json::json!(item)
    );
    assert_eq!(open[0].source_ref, "requests.requests:9100");
    set_finished(&f, item, "failed", 60).await?;
    step(&f).await?;
    let closed = entries(&f).await?;
    assert_eq!(closed[0].status, "broken");
    assert_eq!(
        closed[0].closed_by,
        Some(format!("agents.work_items:{item}"))
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_promise_is_kept_when_its_backing_completes_and_a_result_is_reported()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let item = work_item(&f, "running", None).await?;
    promise_turn(&f).await?;
    step(&f).await?;
    set_finished(&f, item, "completed", 60).await?;
    insert_request(&f, &Req::lane(9200, "primary", 90)).await?;
    crate::turn_facts::insert_text_block(&f, 9200, 0, "The suite passed.").await?;
    step(&f).await?;
    let rows = entries(&f).await?;
    assert_eq!(rows[0].status, "kept", "rows: {rows:?}");
    assert_eq!(rows[0].closed_by.as_deref(), Some("requests.requests:9200"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_later_genuine_operator_message_supersedes_a_promise() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    promise_turn(&f).await?;
    step(&f).await?;
    insert_request(
        &f,
        &Req::primary(9300, 120, "never mind, do something else"),
    )
    .await?;
    sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
         SELECT 9300, $1, k.id FROM prompt_kinds.kinds k WHERE k.code = 'operator'",
    )
    .bind(f.session_id)
    .execute(f.handle.pool())
    .await?;
    step(&f).await?;
    let rows = entries(&f).await?;
    assert_eq!(rows[0].status, "superseded", "rows: {rows:?}");
    assert_eq!(rows[0].closed_by.as_deref(), Some("requests.requests:9300"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_promise_before_the_registry_existed_has_no_coverage() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO agents.work_items
             (session_id, wire_tool_use_id, kind, task_id, launched_at, state)
         VALUES ($1, 'toolu_later', 'bash', 'task-9', $2, 'running')",
    )
    .bind(f.session_id)
    .bind(f.base + Duration::hours(1))
    .execute(f.handle.pool())
    .await?;
    promise_turn(&f).await?;
    step(&f).await?;
    let rows = entries(&f).await?;
    assert_eq!(rows[0].data["registry_coverage"], serde_json::json!(false));
    assert!(rows[0].data["backing"].is_null());
    f.db.cleanup().await?;
    Ok(())
}
