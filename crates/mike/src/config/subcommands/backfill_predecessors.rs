use anyhow::{Context, Result};

use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;

pub(super) async fn backfill_predecessors(cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for backfill-predecessors")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "backfill-predecessors requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("backfill-predecessors: could not connect to the target database")?;

    println!("Backfilling predecessor_request_id...");
    let predecessor_rows = handle
        .backfill_set_predecessors()
        .await
        .context("backfill-predecessors: failed to set predecessor_request_id")?;
    println!("  {predecessor_rows} row(s) updated");

    println!("Backfilling request_number...");
    let number_rows = handle
        .backfill_set_request_numbers()
        .await
        .context("backfill-predecessors: failed to set request_number")?;
    println!("  {number_rows} row(s) updated");

    println!("Finding multi-valued-predecessor requests (defect 2)...");
    let multi = handle
        .find_multi_valued_delta_requests()
        .await
        .context("backfill-predecessors: failed to find multi-valued delta requests")?;
    println!("  {} request(s) found", multi.len());

    let mut recomputed = 0usize;
    let mut skipped = 0usize;
    for request_id in &multi {
        let did_recompute = handle
            .backfill_recompute_deltas(*request_id)
            .await
            .with_context(|| {
                format!(
                    "backfill-predecessors: failed to recompute deltas for request {request_id}"
                )
            })?;
        if did_recompute {
            recomputed += 1;
        } else {
            skipped += 1;
        }
    }

    println!(
        "\nbackfill-predecessors summary: {predecessor_rows} predecessor_request_id row(s), \
         {number_rows} request_number row(s), {recomputed} delta recompute(s), {skipped} skipped"
    );
    Ok(())
}
