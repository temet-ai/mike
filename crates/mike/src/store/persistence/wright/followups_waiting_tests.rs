use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::Value;

use super::followups_tests::{CHECK, Env};

async fn waiting_issues(env: &Env) -> anyhow::Result<Vec<i64>> {
    let (query, params): (String, Value) =
        sqlx::query_as("SELECT query, params FROM wright.checks WHERE name = 'wright_followups'")
            .fetch_one(env.persist.pool())
            .await?;
    let rows: Vec<(String, String, Value)> = sqlx::query_as(sqlx::AssertSqlSafe(query))
        .bind(params)
        .fetch_all(env.persist.pool())
        .await?;
    Ok(rows
        .into_iter()
        .filter(|(_, resource, _)| resource == "issues waiting for a fixer")
        .flat_map(|(_, _, evidence)| {
            evidence["issues"]
                .as_array()
                .map(|issues| issues.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .collect())
}

async fn age(env: &Env, id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE wright.followups SET filed_at = now() - interval '4 days' WHERE id = $1")
        .bind(id)
        .execute(env.persist.pool())
        .await?;
    Ok(())
}

#[tokio::test]
async fn followups_waiting_check_skips_a_code_defect_the_repair_owns_and_counts_the_rest()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (problem, episode) = env.problem(CHECK, true).await?;
    let repaired = env.finding(problem, episode).await?;
    let plain = env.finding(problem, episode).await?;
    let defect = env.record("code_defect", Some("wire"), repaired).await?;
    let hardening = env.record("hardening", Some("wire"), plain).await?;
    for id in [defect, hardening] {
        env.file(id).await?;
        age(&env, id).await?;
    }
    sqlx::query(
        "INSERT INTO wright.repairs (finding_id, problem_id, owning_part, state, worktree, branch,
                                     merge_sha, landed_at, deployed_at, healthy_at)
         VALUES ($1, $2, 'wire', 'healthy', '/w', 'b', 'm', now(), now(), now())",
    )
    .bind(repaired)
    .bind(problem)
    .execute(env.persist.pool())
    .await?;
    let issues = waiting_issues(&env).await?;
    assert!(issues.contains(&(9000 + hardening)));
    assert!(!issues.contains(&(9000 + defect)));
    let next = env.persist.web().next_wright_handoff().await?;
    assert_eq!(next.map(|row| row.id), Some(hardening));
    env.db.cleanup().await
}

async fn plan(env: &Env, query: &str, params: Option<Value>) -> anyhow::Result<String> {
    let explain = format!("EXPLAIN (COSTS OFF) {query}");
    let mut statement = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(explain));
    if let Some(params) = params {
        statement = statement.bind(params);
    }
    Ok(statement.fetch_all(env.persist.pool()).await?.join("\n"))
}

#[tokio::test]
async fn followups_waiting_check_and_handoff_plans_have_no_subplan() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let (query, params): (String, Value) =
        sqlx::query_as("SELECT query, params FROM wright.checks WHERE name = 'wright_followups'")
            .fetch_one(env.persist.pool())
            .await?;
    let check = plan(&env, &query, Some(params)).await?;
    assert!(!check.contains("SubPlan"), "{check}");
    let handoff = plan(&env, include_str!("sql/followups/next_handoff.sql"), None).await?;
    assert!(!handoff.contains("SubPlan"), "{handoff}");
    env.db.cleanup().await
}
