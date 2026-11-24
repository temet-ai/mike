use std::collections::BTreeMap;

use anyhow::{Context, Result};

use super::RepairRepeatedMessagesArgs;
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::message_repair::{
    BatchRepair, MessageCopy, find_message_copies, repair_message_copies,
};

pub(super) async fn repair_repeated_messages(
    args: RepairRepeatedMessagesArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for repair-repeated-messages")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!("repair-repeated-messages requires a database_url (set MIKE_DATABASE_URL)");
    };
    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("repair-repeated-messages: could not connect to the target database")?;
    println!("Finding stored copies of messages (one sorted pass over messages.messages)...");
    let copies = find_message_copies(&handle.pool).await?;
    print_months(&copies);
    if !args.execute {
        println!("Dry run: nothing changed. Pass --execute to repoint and delete the copies.");
        return Ok(());
    }
    let mut total = BatchRepair::default();
    for (n, batch) in copies
        .chunks(args.batch_size.max(1))
        .take(args.max_batches)
        .enumerate()
    {
        let repair = repair_message_copies(&handle.pool, batch, args.idle_minutes)
            .await
            .with_context(|| format!("batch {} (copies {}..)", n + 1, batch[0].copy_id))?;
        if !repair.mismatched_requests.is_empty() {
            anyhow::bail!(
                "batch {} rolled back: requests {:?} rebuilt differently; nothing in it changed",
                n + 1,
                repair.mismatched_requests
            );
        }
        add(&mut total, &repair);
        println!(
            "  batch {}: {} of {} copies repaired, {} references moved, {} rows deleted ({} bytes), {} requests checked",
            n + 1,
            repair.repaired,
            repair.copies,
            repair.references_moved,
            repair.rows_deleted,
            repair.bytes_deleted,
            repair.requests_checked
        );
    }
    println!(
        "\nrepaired {} of {} copies: {} references moved, {} rows deleted ({} bytes), {} requests checked, none differed",
        total.repaired,
        total.copies,
        total.references_moved,
        total.rows_deleted,
        total.bytes_deleted,
        total.requests_checked
    );
    Ok(())
}

fn add(total: &mut BatchRepair, repair: &BatchRepair) {
    total.copies += repair.copies;
    total.repaired += repair.repaired;
    total.references_moved += repair.references_moved;
    total.rows_deleted += repair.rows_deleted;
    total.bytes_deleted += repair.bytes_deleted;
    total.requests_checked += repair.requests_checked;
}

fn print_months(copies: &[MessageCopy]) {
    let mut months: BTreeMap<i32, (u64, i64)> = BTreeMap::new();
    for copy in copies {
        let entry = months.entry(copy.month).or_default();
        entry.0 += 1;
        entry.1 += i64::from(copy.byte_size);
    }
    println!("\nmonth    copies      bytes");
    for (month, (count, bytes)) in &months {
        println!(
            "{}-{:02}  {count:>9}  {bytes:>13}",
            month / 100,
            month % 100
        );
    }
    let bytes: i64 = months.values().map(|(_, b)| *b).sum();
    println!("total    {:>9}  {bytes:>13}", copies.len());
}
