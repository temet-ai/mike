use std::time::Instant;

use uuid::Uuid;

use mike::store::persistence::workspace::flow_admission::resolve_registration_session;

use super::input;
use crate::store::workspace::support::{shared_handle, unique};

#[tokio::test]
async fn isolated_registration_resolution_before_after_measurement() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let cwd = unique("flow-admission-perf");
    handle
        .register_workspace_session(input(session, &cwd))
        .await?;
    let mut before = Vec::new();
    let mut after = Vec::new();
    for _ in 0..25 {
        let mut tx = handle.pool().begin().await?;
        let began = Instant::now();
        mike::store::persistence::id_resolvers::resolve_session_id(
            &mut *tx,
            &session.to_string(),
            None,
            None,
        )
        .await?;
        before.push(began.elapsed().as_secs_f64() * 1000.0);
        tx.rollback().await?;
        let mut tx = handle.pool().begin().await?;
        let began = Instant::now();
        resolve_registration_session(&mut tx, &input(session, &cwd)).await?;
        after.push(began.elapsed().as_secs_f64() * 1000.0);
        tx.rollback().await?;
    }
    before.sort_by(f64::total_cmp);
    after.sort_by(f64::total_cmp);
    println!(
        "P02 isolated existing-session resolution: runs=25 before_median_ms={:.3} after_median_ms={:.3}; client elapsed including all helper round trips, shared registration writes excluded",
        before[12], after[12]
    );
    let plan: serde_json::Value = sqlx::query_scalar(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
         SELECT id, flow_capture_version, flow_admitted_at, flow_producer,
                flow_hook_event_name, flow_start_source
         FROM sessions.sessions WHERE session_uuid = $1 FOR UPDATE",
    )
    .bind(session)
    .fetch_one(handle.pool())
    .await?;
    println!("P02 isolated conflict lookup EXPLAIN: {plan}");
    handle.pool().close().await;
    Ok(())
}
