use anyhow::{Context, Result};

use super::SweepOrphanedMessagesArgs;
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;

pub(super) async fn sweep_orphaned_messages(
    args: SweepOrphanedMessagesArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for sweep-orphaned-messages")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "sweep-orphaned-messages requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("sweep-orphaned-messages: could not connect to the target database")?;

    let before = handle
        .count_orphaned_messages()
        .await
        .context("sweep-orphaned-messages: dry-run count failed")?;
    println!(
        "Orphaned messages.messages rows: {} ({} byte(s) reclaimable)",
        before.count, before.total_bytes
    );

    if !args.execute {
        println!("Dry run only — pass --execute to delete. No rows were removed.");
        return Ok(());
    }

    let mut total_removed: u64 = 0;
    let mut batches: u64 = 0;
    loop {
        let removed = handle
            .sweep_orphaned_messages(args.batch_size)
            .await
            .context("sweep-orphaned-messages: batch delete failed")?;
        if removed == 0 {
            break;
        }
        total_removed += removed;
        batches += 1;
        println!("  batch {batches}: removed {removed} row(s) (running total {total_removed})");
    }

    println!(
        "\nsweep-orphaned-messages summary: {total_removed} row(s) removed across {batches} batch(es)"
    );
    Ok(())
}
