use serde_json::json;

use super::outcome_sync_support_tests::{Env, NewRecord, pair};
use crate::store::persistence::test_support::seed_project;

async fn check(env: &Env, run: &str, state: &str, exit: i32) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar!(
        "INSERT INTO launch.tree_checks (run_id, state, command, finished_at, exit_code)
         VALUES ($1, $2, 'make check', now(), $3) RETURNING id",
        run,
        state,
        exit
    )
    .fetch_one(env.handle.pool())
    .await?)
}

async fn triaged(env: &Env, receipt: i64, triage: &str) -> anyhow::Result<i64> {
    let mut record =
        NewRecord::new("fleet_triage", "chosen").verdict(json!({"score": 0.8, "triage": triage}));
    record.check_receipt_id = Some(receipt);
    env.record(record).await
}

#[tokio::test]
async fn a_retry_triage_is_scored_by_the_next_check_of_the_same_tree() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let flaky = check(&env, "run-a", "tree-1", 1).await?;
    check(&env, "run-a", "tree-1", 0).await?;
    let stuck = check(&env, "run-b", "tree-1", 1).await?;
    check(&env, "run-b", "tree-1", 1).await?;
    let changed = check(&env, "run-c", "tree-1", 1).await?;
    check(&env, "run-c", "tree-2", 0).await?;
    let fix = check(&env, "run-d", "tree-1", 1).await?;
    check(&env, "run-d", "tree-1", 0).await?;
    let ids = [
        triaged(&env, flaky, "retry").await?,
        triaged(&env, stuck, "retry").await?,
        triaged(&env, changed, "retry").await?,
        triaged(&env, fix, "fix_code").await?,
    ];
    assert_eq!(env.sync().await?, 2);
    let source = "next_check_same_tree";
    assert_eq!(env.labels(ids[0]).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(ids[1]).await?, vec![pair("wrong", source)]);
    assert!(env.labels(ids[2]).await?.is_empty());
    assert!(env.labels(ids[3]).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn a_merge_suggestion_is_right_once_merged_and_wrong_after_two_weeks_unmerged()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let pool = env.handle.pool();
    let mut projects = Vec::new();
    for n in 0..6 {
        projects.push(seed_project(pool, &format!("/merge/{n}/{}", env.session_uuid)).await?);
    }
    sqlx::query!(
        "UPDATE projects.projects SET merged_into_id = $2, merged_at = now() + interval '1 hour'
          WHERE id = $1",
        projects[0],
        projects[1]
    )
    .execute(pool)
    .await?;
    let mut ids = Vec::new();
    for (pair_at, age) in [(0, 1), (2, 15), (4, 1)] {
        let mut record = NewRecord::new("project_merge", "suggested").verdict(json!({
            "score": 0.9,
            "source_project_id": projects[pair_at],
            "target_project_id": projects[pair_at + 1],
        }));
        record.age_days = age;
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(ids[0]).await?,
        vec![pair("right", "operator_merge")]
    );
    assert_eq!(
        env.labels(ids[1]).await?,
        vec![pair("wrong", "operator_merge")]
    );
    assert!(env.labels(ids[2]).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn a_finished_thread_suggestion_is_right_when_the_thread_moved_on_and_wrong_when_it_did_not()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    for (slug, status) in [
        ("closed", "done"),
        ("stale", "active"),
        ("recent", "active"),
    ] {
        sqlx::query!(
            "INSERT INTO work.threads (project_id, slug, title, status, updated_at)
             VALUES ($1, $2, $2, $3, now() + interval '1 hour')",
            env.project,
            slug,
            status
        )
        .execute(env.handle.pool())
        .await?;
    }
    let mut ids = Vec::new();
    for (slug, age) in [("closed", 1), ("stale", 15), ("recent", 1)] {
        let mut record = NewRecord::new("thread_status", "suggested")
            .verdict(json!({"score": 0.9, "slug": slug}));
        record.project_id = Some(env.project);
        record.age_days = age;
        ids.push(env.record(record).await?);
    }
    assert_eq!(env.sync().await?, 2);
    let source = "operator_status_change";
    assert_eq!(env.labels(ids[0]).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(ids[1]).await?, vec![pair("wrong", source)]);
    assert!(env.labels(ids[2]).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}
