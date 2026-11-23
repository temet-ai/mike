use anyhow::{Context, Result};

use super::BackfillSentContentArgs;
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::sent_content::SentBackfillFailure;

#[derive(Default)]
struct Totals {
    scanned: usize,
    verified: usize,
    failures: Vec<SentBackfillFailure>,
}

pub(super) async fn backfill_sent_content(
    args: BackfillSentContentArgs,
    cli: CliOverrides,
) -> Result<()> {
    if args.batch_size <= 0 {
        anyhow::bail!(
            "backfill-sent-content: --batch-size must be greater than zero, got {}",
            args.batch_size
        );
    }
    let config = load::load(cli).context("failed to resolve config for backfill-sent-content")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "backfill-sent-content requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };
    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("backfill-sent-content: could not connect to the target database")?;

    let mode = if args.execute {
        "writing"
    } else {
        "dry run, writing nothing"
    };
    println!(
        "Storing the sent body as content for rows with old sent patches, request_id={} through {} \
         (batch size {}, {mode})...",
        args.from,
        args.to.map_or_else(|| "end".to_string(), |t| t.to_string()),
        args.batch_size,
    );
    let totals = run_batches(&handle, &args).await?;
    print_summary(&totals, args.execute);
    Ok(())
}

async fn run_batches(handle: &PersistHandle, args: &BackfillSentContentArgs) -> Result<Totals> {
    let mut totals = Totals::default();
    let mut cursor = args.from;
    loop {
        let Some(batch) = handle
            .backfill_sent_content_batch(cursor, args.to, args.batch_size, args.execute)
            .await
            .with_context(|| {
                format!("backfill-sent-content: batch from request_id={cursor} failed")
            })?
        else {
            return Ok(totals);
        };
        totals.scanned += batch.scanned;
        totals.verified += batch.verified;
        totals.failures.extend(batch.failures);
        println!(
            "  through request_id={} ({} scanned, {} verified, {} failed); resume with --from {}",
            batch.watermark,
            totals.scanned,
            totals.verified,
            totals.failures.len(),
            batch.watermark + 1,
        );
        cursor = batch.watermark + 1;
    }
}

fn print_summary(totals: &Totals, execute: bool) {
    let verb = if execute { "stored" } else { "would store" };
    println!(
        "Done. {} row(s) scanned, {} {verb} and verified against request_body plus its patches, \
         {} failed and keep only their old patches.",
        totals.scanned,
        totals.verified,
        totals.failures.len()
    );
    for failure in &totals.failures {
        println!("  request_id={}: {}", failure.request_id, failure.reason);
    }
    if !execute {
        println!("Dry run only — pass --execute to store the content. Nothing was written.");
    }
}
