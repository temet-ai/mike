use anyhow::{Context, Result};

use super::ReclaimSpaceArgs;
use crate::config::{CliOverrides, load};

const RECLAIMABLE_TABLES: &[(&str, &str)] = &[
    (
        "requests.requests",
        "VACUUM (FULL, VERBOSE) requests.requests",
    ),
    ("requests.deltas", "VACUUM (FULL, VERBOSE) requests.deltas"),
    (
        "requests.messages",
        "VACUUM (FULL, VERBOSE) requests.messages",
    ),
    (
        "messages.messages",
        "VACUUM (FULL, VERBOSE) messages.messages",
    ),
    ("tools.results", "VACUUM (FULL, VERBOSE) tools.results"),
    ("tools.uses", "VACUUM (FULL, VERBOSE) tools.uses"),
    ("requests.tools", "VACUUM (FULL, VERBOSE) requests.tools"),
    (
        "responses.responses",
        "VACUUM (FULL, VERBOSE) responses.responses",
    ),
];

fn vacuum_statement(table: &str) -> Result<&'static str> {
    RECLAIMABLE_TABLES
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, stmt)| *stmt)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "reclaim-space: {table} is not a reclaimable table. Allowed: {}",
                RECLAIMABLE_TABLES
                    .iter()
                    .map(|(n, _)| *n)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

pub(super) async fn reclaim_space(args: ReclaimSpaceArgs, cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for reclaim-space")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "reclaim-space requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    vacuum_statement(&args.table)?;

    let pool = sqlx::PgPool::connect(database_url)
        .await
        .context("reclaim-space: could not connect to the target database")?;

    let before_row = sqlx::query!(
        "SELECT pg_size_pretty(pg_total_relation_size(to_regclass($1))) AS \"tbl!\", \
                pg_size_pretty(pg_database_size(current_database())) AS \"db!\"",
        &args.table,
    )
    .fetch_one(&pool)
    .await
    .context("reclaim-space: failed to read the table size")?;
    let (before, db_before) = (before_row.tbl, before_row.db);

    println!("{} is {before} (database {db_before})", args.table);
    if args.dry_run {
        println!("--dry-run: nothing rewritten");
        return Ok(());
    }

    println!(
        "Rewriting {} with VACUUM FULL — it holds an ACCESS EXCLUSIVE lock \
         and the table is unreadable until it finishes.",
        args.table
    );

    sqlx::query!("SET statement_timeout = 0")
        .execute(&pool)
        .await
        .context("reclaim-space: failed to clear statement_timeout")?;
    sqlx::raw_sql(vacuum_statement(&args.table)?)
        .execute(&pool)
        .await
        .with_context(|| format!("reclaim-space: VACUUM FULL {} failed", args.table))?;

    let after_row = sqlx::query!(
        "SELECT pg_size_pretty(pg_total_relation_size(to_regclass($1))) AS \"tbl!\", \
                pg_size_pretty(pg_database_size(current_database())) AS \"db!\"",
        &args.table,
    )
    .fetch_one(&pool)
    .await
    .context("reclaim-space: failed to read the table size after the rewrite")?;
    let (after, db_after) = (after_row.tbl, after_row.db);

    println!(
        "\nreclaim-space summary: {} {before} -> {after}",
        args.table
    );
    println!("database {db_before} -> {db_after}");
    Ok(())
}
