use anyhow::{Context, Result};

use crate::config::persist_bootstrap::primary_tcp_unreachable;
use crate::config::{CliOverrides, load};
use crate::store::persistence::{PersistError, migrations, pool};

pub(super) async fn migrate_check(cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for migrate-check")?;
    let Some(primary_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "migrate-check requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };
    let fallback_eligible = config.db_fallback_enabled && config.local_database_url.is_some();

    let pool = resolve_target_pool(
        primary_url,
        fallback_eligible,
        config.local_database_url.as_deref(),
    )
    .await
    .context("migrate-check: could not connect to the target database")?;
    let assessment = migrations::assess(&pool).await;
    pool.close().await;
    let assessment = assessment.context("migrate-check: assessment query failed")?;

    println!(
        "Migration assessment — {} pending, {} mismatched, {} missing-from-binary, {} \
         out-of-order",
        assessment.pending.len(),
        assessment.mismatched.len(),
        assessment.missing_from_binary.len(),
        assessment.out_of_order.len(),
    );

    if !assessment.out_of_order.is_empty() {
        println!(
            "\n✗ BLOCKING — {} pending migration(s) sort BELOW a version already applied to \
             this database; sqlx applies pending migrations in ascending version order \
             regardless of what's already applied, so it would run silently — but in a \
             different relative order than on a fresh database:",
            assessment.out_of_order.len()
        );
        for m in &assessment.out_of_order {
            println!(
                "    {} {} would apply AFTER {} (already applied) — a fresh DB would run it \
                 BEFORE",
                m.version, m.description, m.max_applied
            );
        }
        println!(
            "  → Renumber the out-of-order migration(s) to a version above the highest \
             applied version before deploying."
        );
    }

    if !assessment.mismatched.is_empty() {
        println!(
            "\n✗ BLOCKING — {} applied migration(s) were modified after being applied \
             (checksum mismatch); sqlx will refuse to migrate:",
            assessment.mismatched.len()
        );
        for m in &assessment.mismatched {
            println!(
                "    {} {}  (binary {} != db {})",
                m.version, m.description, m.binary_checksum, m.db_checksum
            );
        }
        println!(
            "  → An applied migration's .sql changed. Restore the original file, or reconcile \
             _sqlx_migrations, before deploying."
        );
    }

    if !assessment.missing_from_binary.is_empty() {
        println!(
            "\n✗ BLOCKING — {} migration(s) recorded in the database are absent from this binary \
             (binary older than the DB schema):",
            assessment.missing_from_binary.len()
        );
        for v in &assessment.missing_from_binary {
            println!("    {v}");
        }
        println!("  → Deploy a newer build that includes these migrations.");
    }

    if !assessment.pending.is_empty() {
        println!("\nPending (will be applied on next migrate):");
        for v in &assessment.pending {
            println!("    {v}");
        }
    }

    if assessment.is_blocking() {
        anyhow::bail!("migration assessment FAILED — target DB untouched, deploy must abort");
    }

    println!(
        "\n✓ OK — embedded migrations are compatible with the target; migrate will apply cleanly."
    );
    Ok(())
}

async fn resolve_target_pool(
    primary_url: &str,
    fallback_eligible: bool,
    local_url: Option<&str>,
) -> Result<sqlx::PgPool, PersistError> {
    if fallback_eligible && primary_tcp_unreachable(primary_url).await {
        println!(
            "==> primary database unreachable (fast probe) — assessing the local fallback DB \
             instead (this boot would use it too)"
        );
        return pool::build_pool(local_url.ok_or(PersistError::MissingDatabaseUrl)?).await;
    }

    match pool::build_pool(primary_url).await {
        Ok(pool) => Ok(pool),
        Err(PersistError::Connect { .. }) if fallback_eligible => {
            println!(
                "==> primary database unreachable — assessing the local fallback DB instead \
                 (this boot would use it too)"
            );
            pool::build_pool(local_url.ok_or(PersistError::MissingDatabaseUrl)?).await
        }
        Err(e) => Err(e),
    }
}

const RETRACTED_MIGRATION_VERSIONS: [i64; 3] = [20260722100000, 20260722300000, 20260722700000];

pub(super) async fn forget_retracted_migrations(cli: CliOverrides) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for forget-retracted-migrations")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "forget-retracted-migrations requires a database_url — persistence is not \
             configured (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let pool = sqlx::PgPool::connect(database_url)
        .await
        .context("forget-retracted-migrations: could not connect to the target database")?;

    let live = sqlx::query!(
        "SELECT to_regclass('requests.envelopes') IS NOT NULL AS \"table_exists!\",
                EXISTS (
                    SELECT 1 FROM information_schema.columns
                     WHERE table_schema = 'requests'
                       AND table_name   = 'requests'
                       AND column_name  = 'envelope_id'
                ) AS \"column_exists!\""
    )
    .fetch_one(&pool)
    .await
    .context("forget-retracted-migrations: failed to check whether the schema is gone")?;

    if live.table_exists || live.column_exists {
        anyhow::bail!(
            "refusing to erase ledger rows: requests.envelopes present={}, \
             requests.requests.envelope_id present={} — the schema still exists, so \
             _sqlx_migrations is correct to record it",
            live.table_exists,
            live.column_exists,
        );
    }

    let versions = RETRACTED_MIGRATION_VERSIONS.to_vec();
    let present = sqlx::query_scalar!(
        "SELECT version FROM _sqlx_migrations WHERE version = ANY($1::bigint[]) ORDER BY version",
        &versions,
    )
    .fetch_all(&pool)
    .await
    .context("forget-retracted-migrations: failed to read the migration ledger")?;

    if present.is_empty() {
        println!("Nothing to do — none of the retracted versions are in _sqlx_migrations.");
        return Ok(());
    }

    for version in &present {
        println!("  erasing ledger row {version}");
    }

    let deleted = sqlx::query!(
        "DELETE FROM _sqlx_migrations WHERE version = ANY($1::bigint[])",
        &versions,
    )
    .execute(&pool)
    .await
    .context("forget-retracted-migrations: DELETE failed")?;

    println!(
        "\nforget-retracted-migrations summary: {} ledger row(s) erased",
        deleted.rows_affected()
    );
    Ok(())
}
