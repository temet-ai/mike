use serde_json::{Value, json};
use uuid::Uuid;

use super::outcome_sync_support_tests::{Env, pair};

struct Fleet {
    id: i64,
    run: String,
}

async fn fleet(env: &Env, state: &str) -> anyhow::Result<Fleet> {
    let run = format!("run-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO launch.runs (run_id, backend) VALUES ($1, 'herdr')")
        .bind(&run)
        .execute(env.handle.pool())
        .await?;
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO launch.fleets (slug, project_path, state, run_id)
         VALUES ($1, '/tmp/outcomes', $2, $1) RETURNING id",
    )
    .bind(&run)
    .bind(state)
    .fetch_one(env.handle.pool())
    .await?;
    Ok(Fleet { id, run })
}

async fn let_through(env: &Env, fleet: &Fleet, task: Option<&str>) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "INSERT INTO decisions.records
             (created_at, point, mode, question_set, question_set_hash, model_requested, state,
              questions, request_body, outcome, fleet_run_id, composed, applied)
         VALUES (now() - interval '1 hour', 'fleet_stop_check', 'on', 'test_v1', 'h',
                 'typesafe/jev-1.14', '{}'::jsonb, '{}'::jsonb, '{}'::jsonb, 'ok', $1, $2,
                 'let_through')
         RETURNING id",
    )
    .bind(fleet.id)
    .bind(json!({"verdict": {"score": 0.9, "task": task, "decision": "let_through"}}))
    .fetch_one(env.handle.pool())
    .await?)
}

async fn event(
    env: &Env,
    fleet: &Fleet,
    task: &str,
    kind: &str,
    payload: Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO launch.fleet_events (run_id, task, kind, payload, occurred_at)
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(&fleet.run)
    .bind(task)
    .bind(kind)
    .bind(payload)
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_let_through_is_scored_by_what_the_task_did_next() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let running = fleet(&env, "running").await?;
    let finished = fleet(&env, "done").await?;
    let failed_verdict = let_through(&env, &running, Some("T1")).await?;
    let floor = let_through(&env, &running, Some("T2")).await?;
    let passed = let_through(&env, &running, Some("T3")).await?;
    let open = let_through(&env, &running, Some("T4")).await?;
    let done_quiet = let_through(&env, &finished, Some("T5")).await?;
    let done_failed = let_through(&env, &finished, Some("T6")).await?;
    let untasked = let_through(&env, &running, None).await?;
    event(
        &env,
        &running,
        "T1",
        "verdict_recorded",
        json!({"passed": false}),
    )
    .await?;
    event(&env, &running, "T2", "floor_failed", json!({})).await?;
    event(
        &env,
        &running,
        "T3",
        "verdict_recorded",
        json!({"passed": true}),
    )
    .await?;
    event(&env, &running, "T4", "report_filed", json!({})).await?;
    event(&env, &finished, "T6", "floor_failed", json!({})).await?;
    let source = "next_task_state";
    assert_eq!(env.sync().await?, 5);
    assert_eq!(
        env.labels(failed_verdict).await?,
        vec![pair("wrong", source)]
    );
    assert_eq!(env.labels(floor).await?, vec![pair("wrong", source)]);
    assert_eq!(env.labels(passed).await?, vec![pair("right", source)]);
    assert!(env.labels(open).await?.is_empty());
    assert_eq!(env.labels(done_quiet).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(done_failed).await?, vec![pair("wrong", source)]);
    assert!(env.labels(untasked).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}
