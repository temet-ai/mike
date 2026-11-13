use serde_json::{Value, json};

use crate::steerer::rollout_params::{RolloutParams, SteererClass};
use crate::store::persistence::{PersistHandle, test_support};

const EFFICIENCY_STEERER_CLASS_MIGRATION: &str =
    include_str!("../../../../../mike-base/src/store/persistence/migrations/20260928070000_efficiency_steerer_class.sql");

const EFFICIENCY: [&str; 5] = [
    "no-full-spill-read",
    "no-cat-in-bash",
    "ast-grep-over-grep",
    "monitor-over-sleep",
    "tee-then-slice-gates",
];

async fn config_of(pool: &sqlx::PgPool, name: &str) -> anyhow::Result<Value> {
    let config = sqlx::query_scalar::<_, Value>(
        "SELECT config FROM steering.steerers WHERE name = $1 AND deleted_at IS NULL",
    )
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(config)
}

async fn apply_migration(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::raw_sql(EFFICIENCY_STEERER_CLASS_MIGRATION)
        .execute(pool)
        .await?;
    Ok(())
}

async fn assert_classified(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    for name in EFFICIENCY {
        let params = RolloutParams::from_json(&config_of(pool, name).await?);
        assert_eq!(params.class, SteererClass::Efficiency, "{name}");
    }
    let ast_grep = RolloutParams::from_json(&config_of(pool, "ast-grep-over-grep").await?);
    assert_eq!(ast_grep.requires.len(), 3);
    let verifier = RolloutParams::from_json(&config_of(pool, "verifier").await?);
    assert_eq!(verifier.class, SteererClass::Correction);
    Ok(())
}

#[tokio::test]
async fn efficiency_steerer_class_migration_classifies_a_fresh_database() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    assert_classified(handle.pool()).await?;

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn efficiency_steerer_class_migration_keeps_an_operator_override() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();

    sqlx::query(
        "UPDATE steering.steerers SET config = config || $1 WHERE name = 'monitor-over-sleep'",
    )
    .bind(json!({"class": "correction"}))
    .execute(pool)
    .await?;
    apply_migration(pool).await?;

    let params = RolloutParams::from_json(&config_of(pool, "monitor-over-sleep").await?);
    assert_eq!(params.class, SteererClass::Correction);

    db.cleanup().await?;
    Ok(())
}
