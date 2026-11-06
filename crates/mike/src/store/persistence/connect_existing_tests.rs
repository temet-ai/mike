use super::{PersistError, PersistHandle};
use crate::store::persistence::test_support as test_util;

async fn ledger_versions(pool: &sqlx::PgPool) -> anyhow::Result<Vec<i64>> {
    Ok(
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await?,
    )
}

#[tokio::test]
async fn connect_existing_accepts_a_current_schema() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    PersistHandle::connect(&db.database_url).await?;

    let handle = PersistHandle::connect_existing(&db.database_url).await?;
    handle.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn connect_existing_refuses_pending_migrations_and_applies_none() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let newest = *ledger_versions(handle.pool())
        .await?
        .last()
        .ok_or_else(|| anyhow::anyhow!("template DB has an empty migration ledger"))?;
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
        .bind(newest)
        .execute(handle.pool())
        .await?;
    let before = ledger_versions(handle.pool()).await?;

    let refused = PersistHandle::connect_existing(&db.database_url).await;

    match refused {
        Err(PersistError::SchemaNotCurrent(reason)) => {
            assert!(reason.contains(&newest.to_string()), "reason: {reason}");
        }
        other => anyhow::bail!("expected SchemaNotCurrent, got {other:?}"),
    }
    assert_eq!(
        ledger_versions(handle.pool()).await?,
        before,
        "a refused non-migrating connect must not touch the ledger"
    );
    handle.pool().close().await;
    db.cleanup().await
}
