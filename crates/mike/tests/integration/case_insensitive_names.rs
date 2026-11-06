use crate::test_db;

use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

async fn insert_agent(pool: &sqlx::PgPool, name: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO agents.agents (name) VALUES ($1)")
        .bind(name)
        .execute(pool)
        .await
        .map(|_| ())
}

async fn insert_pattern(pool: &sqlx::PgPool, name: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO rules.volatile_line_patterns
                (name, pattern, applies_to_block_index)
              VALUES ($1, '.*', 0)",
    )
    .bind(name)
    .execute(pool)
    .await
    .map(|_| ())
}

fn expect_unique_violation(result: sqlx::Result<()>, markers: &[&str], what: &str) {
    let Err(err) = result else {
        panic!("{what} must be rejected by the unique surface");
    };
    let msg = err.to_string();
    assert!(
        markers.iter().any(|m| msg.contains(m)),
        "expected unique-violation citing one of {markers:?}, got: {msg}"
    );
}

#[tokio::test]
async fn case_insensitive_uniqueness_covers_agents_and_volatile_patterns() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping case_insensitive_uniqueness_covers_agents_and_volatile_patterns"
        );
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let suffix = uuid::Uuid::new_v4();
    let base = format!("frontend-developer-{suffix}");
    insert_agent(&pool, &base).await?;
    expect_unique_violation(
        insert_agent(&pool, &format!("Frontend-Developer-{suffix}")).await,
        &["uq_agents_lower_name"],
        "case-only duplicate agent",
    );

    let alpha = format!("alpha-agent-{suffix}");
    let beta = format!("beta-agent-{suffix}");
    let gamma = format!("gamma-AGENT-{suffix}");
    for name in [&alpha, &beta, &gamma] {
        insert_agent(&pool, name).await?;
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM agents.agents
          WHERE name = ANY($1::TEXT[])",
    )
    .bind(&[alpha, beta, gamma])
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 3, "three distinct names must all be accepted");

    insert_pattern(&pool, &format!("billing-headers-{suffix}")).await?;
    expect_unique_violation(
        insert_pattern(&pool, &format!("Billing-Headers-{suffix}")).await,
        &["uq_volatile_line_patterns_lower_name"],
        "case-only duplicate pattern",
    );

    let exact = format!("exact-name-{suffix}");
    insert_agent(&pool, &exact).await?;
    expect_unique_violation(
        insert_agent(&pool, &exact).await,
        &["agents_name_key", "uq_agents_lower_name"],
        "exact-name duplicate agent",
    );

    test_db.cleanup().await?;
    Ok(())
}
